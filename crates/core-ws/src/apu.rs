// SPDX-License-Identifier: GPL-3.0-or-later
//! WonderSwan sound (APU) components.
//!
//! Built subsystem-by-subsystem, starting with the pieces that fix documented
//! community bugs in isolation (see `docs/COMMUNITY-BUGS.md`).

/// The channel-4 noise generator: a 15-bit LFSR with 8 selectable tap positions.
///
/// Reproduces the WonderSwan noise algorithm from WSMan: each tick,
/// `bit = 1 ^ (ctr>>7) ^ (ctr>>tap)`, then `ctr = ((ctr<<1) | bit) & 0x7FFF`.
/// The leading `1 ^ …` is what stops the all-zero state from locking up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoiseLfsr {
    counter: u16,
    tap: u8,
}

impl NoiseLfsr {
    /// Tap position for each of the 8 noise tap modes (WSMan).
    pub const TAPS: [u8; 8] = [14, 10, 13, 4, 8, 6, 9, 11];

    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter: 0,
            tap: Self::TAPS[0],
        }
    }

    /// Select the tap mode (`0..=7`).
    pub const fn set_mode(&mut self, mode: u8) {
        self.tap = Self::TAPS[(mode & 7) as usize];
    }

    /// Advance the LFSR one step.
    pub const fn tick(&mut self) {
        let bit = (1 ^ (self.counter >> 7) ^ (self.counter >> self.tap)) & 1;
        self.counter = ((self.counter << 1) | bit) & 0x7FFF;
    }

    /// Current 15-bit LFSR state.
    #[must_use]
    pub const fn state(&self) -> u16 {
        self.counter
    }

    /// Current noise output bit (LSB of the LFSR).
    #[must_use]
    pub const fn output_bit(&self) -> bool {
        self.counter & 1 != 0
    }

    /// Reset the LFSR counter to 0 (`SND_NOISE` $8E bit 3 write). Not a lock-up:
    /// the `1 ^ …` feedback advances it away from 0 on the next tick.
    pub const fn reset(&mut self) {
        self.counter = 0;
    }
}

impl Default for NoiseLfsr {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel 4: the noise generator plus its control flags.
///
/// **Community bug #4:** the LFSR advance must be **independent of the
/// wave/noise output-select bit** (`SND_CTRL` $90 bit 7). Games such as
/// *Clock Tower* read the running LFSR (as a PRNG / timing source) and hang if
/// an emulator freezes it whenever noise *output* is off. The advance is,
/// however, still gated by the **channel-4 enable** ($90 bit 3) and the
/// **noise/LFSR-update** bit ($8E bit 4) — matching ares. So `tick` advances
/// the LFSR iff `channel_enable && lfsr_update_enable`, never keyed on
/// `wave_mode`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoiseChannel {
    lfsr: NoiseLfsr,
    /// Output select ($90 bit 7): true = wavetable output. Does NOT gate the
    /// LFSR — that is the fix.
    pub wave_mode: bool,
    /// Channel-4 enable (`SND_CTRL` $90 bit 3).
    pub channel_enable: bool,
    /// Noise/LFSR-update enable (`SND_NOISE` $8E bit 4).
    pub lfsr_update_enable: bool,
}

impl NoiseChannel {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lfsr: NoiseLfsr::new(),
            wave_mode: false,
            channel_enable: false,
            lfsr_update_enable: false,
        }
    }

    pub const fn set_mode(&mut self, mode: u8) {
        self.lfsr.set_mode(mode);
    }

    /// Advance the LFSR one step, iff channel-4 and LFSR-update are both enabled
    /// — independent of `wave_mode`.
    pub const fn tick(&mut self) {
        if self.channel_enable && self.lfsr_update_enable {
            self.lfsr.tick();
        }
    }

    /// Reset the LFSR to 0 (`SND_NOISE` $8E bit 3 write).
    pub const fn reset_lfsr(&mut self) {
        self.lfsr.reset();
    }

    #[must_use]
    pub const fn lfsr_state(&self) -> u16 {
        self.lfsr.state()
    }

    /// Audible noise output: the LFSR bit, silenced while in wave mode.
    #[must_use]
    pub const fn noise_output(&self) -> bool {
        !self.wave_mode && self.lfsr.output_bit()
    }
}

impl Default for NoiseChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lfsr_is_deterministic_and_advances() {
        let mut a = NoiseLfsr::new();
        let mut b = NoiseLfsr::new();
        for _ in 0..1000 {
            a.tick();
            b.tick();
        }
        assert_eq!(a, b, "same seed and taps produce the same sequence");
        assert_ne!(a.state(), NoiseLfsr::new().state(), "state advanced");
    }

    #[test]
    fn all_zero_state_does_not_lock_up() {
        let mut l = NoiseLfsr::new(); // counter starts at 0
        l.tick();
        assert_ne!(l.state(), 0, "the leading 1^ keeps 0 from locking");
    }

    #[test]
    fn different_taps_diverge() {
        let mut a = NoiseLfsr::new();
        a.set_mode(0); // tap 14
        let mut b = NoiseLfsr::new();
        b.set_mode(3); // tap 4
        for _ in 0..50 {
            a.tick();
            b.tick();
        }
        assert_ne!(a.state(), b.state());
    }

    #[test]
    fn state_stays_within_15_bits() {
        let mut l = NoiseLfsr::new();
        for _ in 0..10_000 {
            l.tick();
            assert!(l.state() <= 0x7FFF);
        }
    }

    /// Community bug #4: *Clock Tower*. With channel 4 and the LFSR-update bit
    /// enabled, the LFSR keeps running even in wave mode (output-select on);
    /// freezing it there hangs the game.
    #[test]
    fn lfsr_keeps_running_in_wave_mode() {
        let mut ch = NoiseChannel::new();
        ch.channel_enable = true;
        ch.lfsr_update_enable = true;
        ch.wave_mode = true; // output select, must NOT stop the LFSR
        let before = ch.lfsr_state();
        for _ in 0..100 {
            ch.tick();
        }
        assert_ne!(
            ch.lfsr_state(),
            before,
            "frozen LFSR in wave mode = Clock Tower hang"
        );
        assert!(!ch.noise_output(), "noise output stays silent in wave mode");
    }

    /// The advance IS gated by channel-4-enable and the noise-update bit.
    #[test]
    fn lfsr_frozen_when_channel_or_update_disabled() {
        for (chan, upd) in [(false, true), (true, false), (false, false)] {
            let mut ch = NoiseChannel::new();
            ch.channel_enable = chan;
            ch.lfsr_update_enable = upd;
            let before = ch.lfsr_state();
            for _ in 0..100 {
                ch.tick();
            }
            assert_eq!(ch.lfsr_state(), before, "LFSR frozen while a gate is off");
        }
    }

    #[test]
    fn reset_lfsr_zeros_the_state() {
        let mut ch = NoiseChannel::new();
        ch.channel_enable = true;
        ch.lfsr_update_enable = true;
        for _ in 0..20 {
            ch.tick();
        }
        assert_ne!(ch.lfsr_state(), 0);
        ch.reset_lfsr();
        assert_eq!(ch.lfsr_state(), 0);
    }
}
