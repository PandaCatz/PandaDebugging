//! WonderSwan PPU (display) components.
//!
//! Starting with the sprite-DMA timing, which is where community bug #2 lives.

/// The sprite unit and its OAM→internal-RAM DMA timing.
///
/// **Community bug #2 (deep-dive):** the sprite attribute table (OAM) is copied
/// to internal sprite RAM at the **start of line 142** — not instantly, and not
/// at VBlank — and the copy takes `5 + 2n` cycles, during which sprite-table
/// writes are locked out. Emulators that latch instantly (or at VBlank) tear,
/// because games update sprites right around line 142 and rely on the copy
/// window. We model the line-142 trigger, the `5 + 2n` duration, and the write
/// lock; only the frame's *latched* copy is what the display renders.
///
/// The `5 + 2n` value's unit (master vs CPU clock) is the project-wide open
/// cycle-unit question; the *structural* behaviour here — trigger point, lock,
/// and latch — is what actually fixes the tearing and is independent of it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteUnit {
    oam: Vec<u8>,
    latched: Vec<u8>,
    dma_remaining: u32,
}

impl SpriteUnit {
    /// 128 sprites × 4 bytes.
    pub const OAM_BYTES: usize = 512;
    /// Words transferred by the DMA (128 sprites × 2 words).
    pub const OAM_WORDS: u32 = 256;
    /// The scanline on which the sprite DMA starts.
    pub const DMA_SCANLINE: u32 = 142;

    #[must_use]
    pub fn new() -> Self {
        Self {
            oam: vec![0; Self::OAM_BYTES],
            latched: vec![0; Self::OAM_BYTES],
            dma_remaining: 0,
        }
    }

    /// Cycles the line-142 DMA takes: `5 + 2n`.
    #[must_use]
    pub const fn dma_cycles() -> u32 {
        5 + 2 * Self::OAM_WORDS
    }

    /// CPU write to the sprite attribute table. **Locked out while the DMA runs.**
    pub fn write_oam(&mut self, index: usize, value: u8) {
        if self.dma_remaining == 0 && index < Self::OAM_BYTES {
            self.oam[index] = value;
        }
    }

    #[must_use]
    pub fn read_oam(&self, index: usize) -> u8 {
        self.oam.get(index).copied().unwrap_or(0)
    }

    /// Call at the start of each scanline. On line 142 this latches OAM into the
    /// internal copy and starts the `5 + 2n`-cycle DMA that locks the table.
    pub fn on_scanline_start(&mut self, line: u32) {
        if line == Self::DMA_SCANLINE {
            self.latched.copy_from_slice(&self.oam);
            self.dma_remaining = Self::dma_cycles();
        }
    }

    /// Advance the in-progress DMA by `cycles`; the table unlocks on completion.
    pub const fn advance(&mut self, cycles: u32) {
        self.dma_remaining = self.dma_remaining.saturating_sub(cycles);
    }

    /// Whether the sprite table is currently locked (DMA in progress).
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.dma_remaining > 0
    }

    /// The sprite byte the display renders this frame (the latched copy).
    #[must_use]
    pub fn rendered_oam(&self, index: usize) -> u8 {
        self.latched.get(index).copied().unwrap_or(0)
    }
}

impl Default for SpriteUnit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Community bug #2: the latch happens at line 142, not before.
    #[test]
    fn sprite_dma_latches_at_line_142() {
        let mut u = SpriteUnit::new();
        u.write_oam(0, 0xAB);
        u.on_scanline_start(141);
        assert_eq!(u.rendered_oam(0), 0x00, "not latched before line 142");
        u.on_scanline_start(142);
        assert_eq!(u.rendered_oam(0), 0xAB, "latched at line 142");
    }

    /// Community bug #2: the DMA takes 5+2n cycles and locks the table; writes
    /// during the window are ignored (the instant-DMA behaviour that tears).
    #[test]
    fn dma_takes_5_plus_2n_cycles_and_locks_writes() {
        let mut u = SpriteUnit::new();
        u.on_scanline_start(142);
        assert!(u.is_locked());
        assert_eq!(SpriteUnit::dma_cycles(), 517);

        u.write_oam(0, 0xFF);
        assert_eq!(u.read_oam(0), 0x00, "OAM write locked out during the DMA");

        u.advance(SpriteUnit::dma_cycles() - 1);
        assert!(u.is_locked());
        u.advance(1);
        assert!(!u.is_locked(), "unlocked after 5+2n cycles");

        u.write_oam(0, 0x55);
        assert_eq!(u.read_oam(0), 0x55, "writes apply once unlocked");
    }

    /// Sprites updated just before line 142 render this frame — no tearing from
    /// a stale/instant latch.
    #[test]
    fn pre_142_updates_render_this_frame() {
        let mut u = SpriteUnit::new();
        u.write_oam(4, 0x11);
        u.on_scanline_start(142);
        assert_eq!(u.rendered_oam(4), 0x11);
    }
}
