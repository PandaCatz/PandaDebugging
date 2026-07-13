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
}

impl Default for NoiseLfsr {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel 4: the noise generator plus its wave/noise mode flag.
///
/// **Community bug #4 (deep-dive):** the LFSR must keep advancing even when
/// channel 4 is in *wave* mode. Games such as *Clock Tower* seed a PRNG from the
/// running LFSR and hang if an emulator freezes it when noise output is off.
/// So [`tick`](NoiseChannel::tick) advances the LFSR unconditionally; only the
/// audible output is gated by the mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoiseChannel {
    lfsr: NoiseLfsr,
    /// When true, channel 4 emits its wavetable instead of noise. The LFSR still
    /// runs — that is the fix.
    pub wave_mode: bool,
}

impl NoiseChannel {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lfsr: NoiseLfsr::new(),
            wave_mode: false,
        }
    }

    pub const fn set_mode(&mut self, mode: u8) {
        self.lfsr.set_mode(mode);
    }

    /// Advance one noise step. Runs the LFSR regardless of `wave_mode`.
    pub const fn tick(&mut self) {
        self.lfsr.tick();
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

    /// Community bug #4: *Clock Tower*. The LFSR must keep running even when
    /// channel 4 is in wave mode; freezing it hangs the game.
    #[test]
    fn lfsr_keeps_running_in_wave_mode() {
        let mut ch = NoiseChannel::new();
        ch.wave_mode = true;
        let before = ch.lfsr_state();
        for _ in 0..100 {
            ch.tick();
        }
        assert_ne!(
            ch.lfsr_state(),
            before,
            "a frozen LFSR in wave mode is the Clock Tower hang"
        );
        assert!(!ch.noise_output(), "noise output stays silent in wave mode");
    }
}
