// SPDX-License-Identifier: GPL-3.0-or-later
//! WonderSwan PPU (display) components.

/// The sprite unit's OAM double-buffering.
///
/// **Community bug #2 (tearing):** the sprites shown in a frame are a *snapshot*
/// of the attribute table, double-buffered so a game can update OAM for the next
/// frame without tearing the current one. Emulators that render live OAM — or
/// latch a single buffer and show it the *same* frame — tear, or show sprite
/// updates a frame early.
///
/// What is confirmed (WSMan / WSdev / ares / Mednafen): a copy of OAM — held in
/// internal work RAM, **not** cartridge SRAM — is taken near the bottom of the
/// visible area and used for the **next** frame's sprites. Two things are *not*
/// settled and are deliberately left to the PPU scheduler (to be pinned by a
/// hardware test rather than guessed):
/// - the exact copy scanline — WSMan and Mednafen say line 142, WSdev and ares
///   say 144;
/// - the copy duration and the accompanying CPU pause — WSMan states the timing
///   is genuinely unknown (the "~256 dot-clocks" figure is a WSdev/ares
///   estimate, not a confirmed value).
///
/// So this models the confirmed structure — double-buffering and the next-frame
/// latch — and nothing it cannot back with a source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteUnit {
    oam: Vec<u8>,
    staged: Vec<u8>,
    displayed: Vec<u8>,
}

impl SpriteUnit {
    /// OAM size: 128 sprites × 4 bytes.
    pub const OAM_BYTES: usize = 512;
    /// Copy scanline per WSMan / Mednafen (WSdev / ares use 144) — open question.
    pub const COPY_SCANLINE_WSMAN: u32 = 142;

    #[must_use]
    pub fn new() -> Self {
        Self {
            oam: vec![0; Self::OAM_BYTES],
            staged: vec![0; Self::OAM_BYTES],
            displayed: vec![0; Self::OAM_BYTES],
        }
    }

    /// CPU write to the live sprite attribute table. Always writable — the
    /// current frame renders the `displayed` snapshot, not this buffer, so a
    /// mid-frame write cannot tear the frame in progress.
    pub fn write_oam(&mut self, index: usize, value: u8) {
        if index < Self::OAM_BYTES {
            self.oam[index] = value;
        }
    }

    #[must_use]
    pub fn read_oam(&self, index: usize) -> u8 {
        self.oam.get(index).copied().unwrap_or(0)
    }

    /// At the sprite-copy scanline: snapshot the live OAM for the *next* frame.
    pub fn snapshot_for_next_frame(&mut self) {
        self.staged.copy_from_slice(&self.oam);
    }

    /// At the start of a frame: the staged snapshot becomes the displayed set.
    pub fn begin_frame(&mut self) {
        self.displayed.copy_from_slice(&self.staged);
    }

    /// The sprite byte the current frame renders (the double-buffered snapshot).
    #[must_use]
    pub fn rendered_oam(&self, index: usize) -> u8 {
        self.displayed.get(index).copied().unwrap_or(0)
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

    /// Community bug #2: sprites are double-buffered — a mid-frame OAM write does
    /// not tear the current frame, and appears the NEXT frame (not same-frame).
    #[test]
    fn sprites_double_buffer_to_the_next_frame() {
        let mut u = SpriteUnit::new();
        u.write_oam(0, 0xAA);
        u.snapshot_for_next_frame(); // copy line of frame N-1
        u.begin_frame(); // frame N displays the snapshot
        assert_eq!(u.rendered_oam(0), 0xAA);

        // A mid-frame-N update must not affect the current frame.
        u.write_oam(0, 0xBB);
        assert_eq!(
            u.rendered_oam(0),
            0xAA,
            "mid-frame OAM write does not tear this frame"
        );

        // Frame N's copy line snapshots the new value; frame N+1 shows it.
        u.snapshot_for_next_frame();
        u.begin_frame();
        assert_eq!(u.rendered_oam(0), 0xBB, "the update appears the next frame");
    }

    #[test]
    fn live_oam_is_independent_of_the_display_buffer() {
        let mut u = SpriteUnit::new();
        u.write_oam(4, 0x11);
        assert_eq!(u.read_oam(4), 0x11);
        assert_eq!(
            u.rendered_oam(4),
            0x00,
            "not displayed until snapshot + frame start"
        );
    }

    #[test]
    fn out_of_range_access_is_safe() {
        let mut u = SpriteUnit::new();
        u.write_oam(9999, 0x55); // ignored
        assert_eq!(u.read_oam(9999), 0);
    }
}
