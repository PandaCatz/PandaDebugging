#![forbid(unsafe_code)]

//! Deterministic test harness for the WonderSwan contracts.
//!
//! [`SyntheticCore`] is *not* WonderSwan emulation. It exercises the shared
//! contract and the headless capture path: it emits a fixed, reproducible stream
//! of video/audio events on the integer clock so tests can prove determinism,
//! split-run equivalence, reset behaviour, and stable output hashing before any
//! real hardware core exists.
//!
//! The hardware-test-ROM runner (loads a homebrew test ROM, runs the core
//! headless, and captures its serial/framebuffer signature) lands together with
//! the V30MZ CPU in Phase 2; see `docs/hardware/` for the validation plan.

use std::collections::VecDeque;
use ws_contracts::{
    AudioPacket, ChannelLayout, ClockRate, ControlId, Core, CoreError, CoreInfo, EmulatedTime,
    InputEvent, InputPortId, InputValue, Model, OutputSink, ResetKind, RunOutcome, RunStopReason,
    VideoField, VideoFrame,
};

const FRAME_PERIOD: u64 = 10;
const AUDIO_PERIOD: u64 = 4;
const SYNTHETIC_MODELS: &[Model] = &[Model::Color];
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Debug)]
pub struct SyntheticCore {
    now: EmulatedTime,
    next_frame: EmulatedTime,
    next_audio: EmulatedTime,
    frame_index: u64,
    audio_index: u64,
    input_pressed: bool,
    pending_inputs: VecDeque<InputEvent>,
}

impl SyntheticCore {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            now: EmulatedTime::from_ticks(0),
            next_frame: EmulatedTime::from_ticks(FRAME_PERIOD),
            next_audio: EmulatedTime::from_ticks(AUDIO_PERIOD),
            frame_index: 0,
            audio_index: 0,
            input_pressed: false,
            pending_inputs: VecDeque::new(),
        }
    }

    fn emit_frame(&mut self, output: &mut dyn OutputSink) -> Result<(), CoreError> {
        let mut pixels = Vec::with_capacity(16);
        for pixel in 0_u8..4 {
            pixels.extend_from_slice(&[
                self.frame_index as u8,
                pixel,
                u8::from(self.input_pressed) * u8::MAX,
                u8::MAX,
            ]);
        }
        let frame = VideoFrame::rgba8888(
            self.next_frame,
            2,
            2,
            (1, 1),
            VideoField::Progressive,
            pixels,
        )?;
        output.video(frame)?;
        self.frame_index = self.frame_index.wrapping_add(1);
        self.next_frame = self.next_frame.checked_add(FRAME_PERIOD)?;
        Ok(())
    }

    fn emit_audio(&mut self, output: &mut dyn OutputSink) -> Result<(), CoreError> {
        let mut samples = Vec::with_capacity(8);
        for frame in 0_i16..4 {
            let value = (self.audio_index as i16)
                .wrapping_mul(16)
                .wrapping_add(frame);
            samples.extend_from_slice(&[value, value.wrapping_neg()]);
        }
        // 24 kHz stereo mirrors the WonderSwan headphone DAC rate.
        let packet = AudioPacket::new(self.next_audio, 24_000, ChannelLayout::Stereo, samples)?;
        output.audio(packet)?;
        self.audio_index = self.audio_index.wrapping_add(1);
        self.next_audio = self.next_audio.checked_add(AUDIO_PERIOD)?;
        Ok(())
    }

    fn apply_input(&mut self, event: InputEvent) {
        let InputValue::Digital(pressed) = event.value else {
            unreachable!("input type is validated before it is queued");
        };
        self.input_pressed = pressed;
    }
}

impl Default for SyntheticCore {
    fn default() -> Self {
        Self::new()
    }
}

impl Core for SyntheticCore {
    fn info(&self) -> CoreInfo {
        CoreInfo {
            system_id: "synthetic-ws-v1",
            display_name: "Deterministic Synthetic Core",
            master_clock: ClockRate {
                numerator_hz: 1_000,
                denominator: 1,
            },
            supported_models: SYNTHETIC_MODELS,
        }
    }

    fn now(&self) -> EmulatedTime {
        self.now
    }

    fn reset(&mut self, _kind: ResetKind) -> Result<(), CoreError> {
        *self = Self::new();
        Ok(())
    }

    fn set_input(&mut self, event: InputEvent) -> Result<(), CoreError> {
        if event.timestamp < self.now {
            return Err(CoreError::TimeWentBackwards {
                now: self.now,
                requested: event.timestamp,
            });
        }
        if event.port != InputPortId(0) || event.control != ControlId(0) {
            return Err(CoreError::UnsupportedInput {
                port: event.port,
                control: event.control,
            });
        }
        match event.value {
            InputValue::Digital(_) => {}
            InputValue::Analog(_) => Err(CoreError::UnsupportedInput {
                port: event.port,
                control: event.control,
            })?,
        }
        if event.timestamp == self.now {
            self.apply_input(event);
        } else {
            let position = self
                .pending_inputs
                .iter()
                .position(|pending| pending.timestamp > event.timestamp)
                .unwrap_or(self.pending_inputs.len());
            self.pending_inputs.insert(position, event);
        }
        Ok(())
    }

    fn run_until(
        &mut self,
        deadline: EmulatedTime,
        output: &mut dyn OutputSink,
    ) -> Result<RunOutcome, CoreError> {
        if deadline < self.now {
            return Err(CoreError::TimeWentBackwards {
                now: self.now,
                requested: deadline,
            });
        }

        loop {
            let next_input = self.pending_inputs.front().map(|event| event.timestamp);
            let next_event = next_input
                .into_iter()
                .chain([self.next_audio, self.next_frame])
                .min()
                .expect("audio and video events always exist");
            if next_event > deadline {
                break;
            }

            if next_input == Some(next_event) {
                self.now = next_event;
                let event = self
                    .pending_inputs
                    .pop_front()
                    .expect("front event supplied its timestamp");
                self.apply_input(event);
            } else if self.next_audio == next_event {
                self.now = self.next_audio;
                self.emit_audio(output)?;
            } else {
                self.now = self.next_frame;
                self.emit_frame(output)?;
            }
        }
        self.now = deadline;
        Ok(RunOutcome {
            timestamp: self.now,
            reason: RunStopReason::DeadlineReached,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSummary {
    pub final_time: EmulatedTime,
    pub video_frames: usize,
    pub audio_packets: usize,
    pub audio_frames: usize,
    pub video_hash: u64,
    pub audio_hash: u64,
    pub event_hash: u64,
}

#[derive(Clone, Debug)]
pub struct CaptureSink {
    video_frames: usize,
    audio_packets: usize,
    audio_frames: usize,
    video_hash: Fnv64,
    audio_hash: Fnv64,
    event_hash: Fnv64,
}

impl CaptureSink {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            video_frames: 0,
            audio_packets: 0,
            audio_frames: 0,
            video_hash: Fnv64::new(),
            audio_hash: Fnv64::new(),
            event_hash: Fnv64::new(),
        }
    }

    #[must_use]
    pub const fn summary(&self, final_time: EmulatedTime) -> CaptureSummary {
        CaptureSummary {
            final_time,
            video_frames: self.video_frames,
            audio_packets: self.audio_packets,
            audio_frames: self.audio_frames,
            video_hash: self.video_hash.finish(),
            audio_hash: self.audio_hash.finish(),
            event_hash: self.event_hash.finish(),
        }
    }
}

impl Default for CaptureSink {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputSink for CaptureSink {
    fn video(&mut self, frame: VideoFrame) -> Result<(), CoreError> {
        self.video_frames = self
            .video_frames
            .checked_add(1)
            .ok_or(CoreError::SizeOverflow)?;
        hash_video(&mut self.video_hash, &frame);
        self.event_hash.write(b"V");
        hash_video(&mut self.event_hash, &frame);
        Ok(())
    }

    fn audio(&mut self, packet: AudioPacket) -> Result<(), CoreError> {
        self.audio_packets = self
            .audio_packets
            .checked_add(1)
            .ok_or(CoreError::SizeOverflow)?;
        self.audio_frames = self
            .audio_frames
            .checked_add(packet.frames())
            .ok_or(CoreError::SizeOverflow)?;
        hash_audio(&mut self.audio_hash, &packet);
        self.event_hash.write(b"A");
        hash_audio(&mut self.event_hash, &packet);
        Ok(())
    }
}

fn hash_video(hash: &mut Fnv64, frame: &VideoFrame) {
    hash.write_u64(frame.timestamp().ticks());
    hash.write_u32(frame.width());
    hash.write_u32(frame.height());
    hash.write_usize(frame.pitch_bytes());
    hash.write(frame.pixels());
}

fn hash_audio(hash: &mut Fnv64, packet: &AudioPacket) {
    hash.write_u64(packet.timestamp().ticks());
    hash.write_u32(packet.sample_rate_hz());
    for sample in packet.samples() {
        hash.write(&sample.to_le_bytes());
    }
}

#[derive(Clone, Copy, Debug)]
struct Fnv64(u64);

impl Fnv64 {
    const fn new() -> Self {
        Self(FNV_OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

/// Run the synthetic core headlessly to `deadline_ticks` and summarise capture.
pub fn run_synthetic(deadline_ticks: u64) -> Result<CaptureSummary, CoreError> {
    let mut core = SyntheticCore::new();
    let mut capture = CaptureSink::new();
    let deadline = EmulatedTime::from_ticks(deadline_ticks);
    let outcome = core.run_until(deadline, &mut capture)?;
    Ok(capture.summary(outcome.timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_is_deterministic_and_measurable() {
        let first = run_synthetic(30).expect("synthetic run succeeds");
        let second = run_synthetic(30).expect("synthetic run succeeds again");
        assert_eq!(first, second, "identical inputs produce identical output");
        assert_eq!(first.video_frames, 3);
        assert_eq!(first.audio_packets, 7);
        assert_eq!(first.audio_frames, 28);
        assert_eq!(first.final_time, EmulatedTime::from_ticks(30));
    }

    #[test]
    fn split_run_matches_single_run() {
        let expected = run_synthetic(30).expect("single run succeeds");
        let mut core = SyntheticCore::new();
        let mut capture = CaptureSink::new();
        core.run_until(EmulatedTime::from_ticks(13), &mut capture)
            .expect("first segment succeeds");
        let outcome = core
            .run_until(EmulatedTime::from_ticks(30), &mut capture)
            .expect("second segment succeeds");
        assert_eq!(capture.summary(outcome.timestamp), expected);
    }

    #[test]
    fn time_cannot_move_backwards() {
        let mut core = SyntheticCore::new();
        let mut capture = CaptureSink::new();
        core.run_until(EmulatedTime::from_ticks(5), &mut capture)
            .expect("forward run succeeds");
        let result = core.run_until(EmulatedTime::from_ticks(4), &mut capture);
        assert!(matches!(result, Err(CoreError::TimeWentBackwards { .. })));
    }

    #[test]
    fn reset_restores_exact_output() {
        let expected = run_synthetic(30).expect("reference run succeeds");
        let mut core = SyntheticCore::new();
        let mut discarded = CaptureSink::new();
        core.run_until(EmulatedTime::from_ticks(17), &mut discarded)
            .expect("pre-reset run succeeds");
        core.reset(ResetKind::Hard).expect("reset succeeds");
        let mut capture = CaptureSink::new();
        let outcome = core
            .run_until(EmulatedTime::from_ticks(30), &mut capture)
            .expect("post-reset run succeeds");
        assert_eq!(capture.summary(outcome.timestamp), expected);
    }

    #[test]
    fn future_input_is_applied_before_outputs_at_the_same_tick() {
        let baseline = run_synthetic(10).expect("baseline run succeeds");
        let mut core = SyntheticCore::new();
        core.set_input(InputEvent {
            timestamp: EmulatedTime::from_ticks(10),
            port: InputPortId(0),
            control: ControlId(0),
            value: InputValue::Digital(true),
        })
        .expect("future input queues");
        let mut capture = CaptureSink::new();
        core.run_until(EmulatedTime::from_ticks(10), &mut capture)
            .expect("run with queued input succeeds");
        let with_input = capture.summary(core.now());
        assert_ne!(with_input.video_hash, baseline.video_hash);
        assert_ne!(with_input.event_hash, baseline.event_hash);
    }
}
