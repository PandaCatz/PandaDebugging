// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

//! Deterministic contracts shared by every WonderSwan crate.
//!
//! The core simulates in exact integer master-clock ticks. It never sleeps,
//! reads wall time, opens host devices, or waits for VSync. Video and audio
//! leave the core as owned, self-describing packets through a caller-owned
//! [`OutputSink`], so a headless test and a windowed frontend observe the exact
//! same event stream.

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Exact position on a core's integer master-clock timeline.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct EmulatedTime(u64);

impl EmulatedTime {
    #[must_use]
    pub const fn from_ticks(ticks: u64) -> Self {
        Self(ticks)
    }

    #[must_use]
    pub const fn ticks(self) -> u64 {
        self.0
    }

    /// Advance by `ticks`, reporting overflow instead of wrapping.
    pub fn checked_add(self, ticks: u64) -> Result<Self, CoreError> {
        self.0
            .checked_add(ticks)
            .map(Self)
            .ok_or(CoreError::TimeOverflow)
    }
}

/// Exact rational clock, e.g. the WonderSwan 12.288 MHz master clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockRate {
    pub numerator_hz: u64,
    pub denominator: u64,
}

impl ClockRate {
    pub fn new(numerator_hz: u64, denominator: u64) -> Result<Self, CoreError> {
        if numerator_hz == 0 || denominator == 0 {
            return Err(CoreError::InvalidMetadata("clock rate must be non-zero"));
        }
        Ok(Self {
            numerator_hz,
            denominator,
        })
    }
}

/// WonderSwan hardware family. There is no NTSC/PAL region concept; behaviour
/// and available I/O differ by SoC generation instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Model {
    /// ASWAN — original mono WonderSwan / Pocket Challenge V2.
    Mono,
    /// SPHINX — WonderSwan Color.
    Color,
    /// SPHINX2 — SwanCrystal.
    Crystal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreInfo {
    pub system_id: &'static str,
    pub display_name: &'static str,
    pub master_clock: ClockRate,
    pub supported_models: &'static [Model],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgba8888,
}

/// The WonderSwan LCD is progressive; the enum keeps room for shared tooling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoField {
    Progressive,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrame {
    timestamp: EmulatedTime,
    width: u32,
    height: u32,
    pitch_bytes: usize,
    pixel_aspect_ratio: (u32, u32),
    field: VideoField,
    format: PixelFormat,
    pixels: Vec<u8>,
}

impl VideoFrame {
    pub fn rgba8888(
        timestamp: EmulatedTime,
        width: u32,
        height: u32,
        pixel_aspect_ratio: (u32, u32),
        field: VideoField,
        pixels: Vec<u8>,
    ) -> Result<Self, CoreError> {
        if width == 0 || height == 0 {
            return Err(CoreError::InvalidMetadata(
                "video dimensions must be non-zero",
            ));
        }
        if pixel_aspect_ratio.0 == 0 || pixel_aspect_ratio.1 == 0 {
            return Err(CoreError::InvalidMetadata(
                "pixel aspect ratio must be non-zero",
            ));
        }

        let pitch_bytes = usize::try_from(width)
            .ok()
            .and_then(|value| value.checked_mul(4))
            .ok_or(CoreError::SizeOverflow)?;
        let expected_len = pitch_bytes
            .checked_mul(usize::try_from(height).map_err(|_| CoreError::SizeOverflow)?)
            .ok_or(CoreError::SizeOverflow)?;
        if pixels.len() != expected_len {
            return Err(CoreError::BufferLength {
                kind: "video",
                expected: expected_len,
                actual: pixels.len(),
            });
        }

        Ok(Self {
            timestamp,
            width,
            height,
            pitch_bytes,
            pixel_aspect_ratio,
            field,
            format: PixelFormat::Rgba8888,
            pixels,
        })
    }

    #[must_use]
    pub const fn timestamp(&self) -> EmulatedTime {
        self.timestamp
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn pitch_bytes(&self) -> usize {
        self.pitch_bytes
    }

    #[must_use]
    pub const fn pixel_aspect_ratio(&self) -> (u32, u32) {
        self.pixel_aspect_ratio
    }

    #[must_use]
    pub const fn field(&self) -> VideoField {
        self.field
    }

    #[must_use]
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelLayout {
    Mono,
    Stereo,
}

impl ChannelLayout {
    #[must_use]
    pub const fn channels(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPacket {
    timestamp: EmulatedTime,
    sample_rate_hz: u32,
    layout: ChannelLayout,
    samples: Vec<i16>,
}

impl AudioPacket {
    pub fn new(
        timestamp: EmulatedTime,
        sample_rate_hz: u32,
        layout: ChannelLayout,
        samples: Vec<i16>,
    ) -> Result<Self, CoreError> {
        if sample_rate_hz == 0 {
            return Err(CoreError::InvalidMetadata("sample rate must be non-zero"));
        }
        if !samples.len().is_multiple_of(layout.channels()) {
            return Err(CoreError::InvalidMetadata(
                "audio samples must contain complete interleaved frames",
            ));
        }
        Ok(Self {
            timestamp,
            sample_rate_hz,
            layout,
            samples,
        })
    }

    #[must_use]
    pub const fn timestamp(&self) -> EmulatedTime {
        self.timestamp
    }

    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    #[must_use]
    pub const fn layout(&self) -> ChannelLayout {
        self.layout
    }

    #[must_use]
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / self.layout.channels()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputPortId(pub u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlId(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputValue {
    Digital(bool),
    Analog(i16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub timestamp: EmulatedTime,
    pub port: InputPortId,
    pub control: ControlId,
    pub value: InputValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetKind {
    Hard,
    Soft,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunStopReason {
    DeadlineReached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunOutcome {
    pub timestamp: EmulatedTime,
    pub reason: RunStopReason,
}

/// Caller-owned destination for a core's timed output.
pub trait OutputSink {
    fn video(&mut self, frame: VideoFrame) -> Result<(), CoreError>;
    fn audio(&mut self, packet: AudioPacket) -> Result<(), CoreError>;
}

/// A runnable machine core. `run_until` advances emulation to `deadline`,
/// emitting mid-frame events through `output`, and reports the exact stop time.
pub trait Core {
    fn info(&self) -> CoreInfo;
    fn now(&self) -> EmulatedTime;
    fn reset(&mut self, kind: ResetKind) -> Result<(), CoreError>;
    fn set_input(&mut self, event: InputEvent) -> Result<(), CoreError>;
    fn run_until(
        &mut self,
        deadline: EmulatedTime,
        output: &mut dyn OutputSink,
    ) -> Result<RunOutcome, CoreError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    TimeWentBackwards {
        now: EmulatedTime,
        requested: EmulatedTime,
    },
    TimeOverflow,
    SizeOverflow,
    BufferLength {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidMetadata(&'static str),
    UnsupportedInput {
        port: InputPortId,
        control: ControlId,
    },
    OutputRejected(&'static str),
}

impl Display for CoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimeWentBackwards { now, requested } => write!(
                formatter,
                "requested tick {} is before current tick {}",
                requested.ticks(),
                now.ticks()
            ),
            Self::TimeOverflow => formatter.write_str("emulated time overflowed"),
            Self::SizeOverflow => formatter.write_str("buffer size overflowed"),
            Self::BufferLength {
                kind,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid {kind} buffer length: expected {expected}, got {actual}"
            ),
            Self::InvalidMetadata(message) => {
                write!(formatter, "invalid output metadata: {message}")
            }
            Self::UnsupportedInput { port, control } => write!(
                formatter,
                "unsupported input port {} control {}",
                port.0, control.0
            ),
            Self::OutputRejected(message) => write!(formatter, "output rejected: {message}"),
        }
    }
}

impl Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_constructor_rejects_wrong_length() {
        let result = VideoFrame::rgba8888(
            EmulatedTime::from_ticks(0),
            2,
            2,
            (1, 1),
            VideoField::Progressive,
            vec![0; 15],
        );
        assert!(matches!(result, Err(CoreError::BufferLength { .. })));
    }

    #[test]
    fn audio_constructor_rejects_partial_stereo_frame() {
        let result = AudioPacket::new(
            EmulatedTime::from_ticks(0),
            24_000,
            ChannelLayout::Stereo,
            vec![0; 3],
        );
        assert!(matches!(result, Err(CoreError::InvalidMetadata(_))));
    }

    #[test]
    fn time_addition_is_checked() {
        let result = EmulatedTime::from_ticks(u64::MAX).checked_add(1);
        assert_eq!(result, Err(CoreError::TimeOverflow));
    }
}
