//! WonderSwan palette models.
//!
//! **Community bug #5 (deep-dive):** the mono WonderSwan uses a *two-stage*
//! palette. `REG_PALMONO_POOL` (`$1C–$1F`) defines an 8-entry shared pool of
//! 4-bit shades (`$0` brightest … `$F` darkest); `REG_PALMONO` (`$20–$3F`) gives
//! 16 palettes that each pick 4 colours *by index into that pool*. Emulators
//! that map palettes straight to 16 greys — skipping the pool indirection —
//! shade many games wrong. The resolved shade is
//! `pool[palette[palette_idx][colour_idx]]`.

/// Mono two-stage palette: an 8-entry shade pool plus 16 four-colour palettes
/// that index into it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonoPalettes {
    /// 8 shade values, each 0..=15 (`$0` brightest, `$F` darkest).
    pool: [u8; 8],
    /// 16 palettes × 4 colours, each a 3-bit index into `pool`.
    palettes: [[u8; 4]; 16],
}

impl MonoPalettes {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pool: [0; 8],
            palettes: [[0; 4]; 16],
        }
    }

    /// Write one of the four `REG_PALMONO_POOL` bytes (`reg` = 0..=3 for
    /// `$1C..=$1F`); each byte holds two 4-bit pool entries.
    pub const fn write_pool(&mut self, reg: usize, value: u8) {
        self.pool[reg * 2] = value & 0x0F;
        self.pool[reg * 2 + 1] = value >> 4;
    }

    /// Write one of the 32 `REG_PALMONO` bytes (`reg` = 0..=31 for `$20..=$3F`);
    /// each byte holds two colours (each a 3-bit pool index).
    pub const fn write_palette(&mut self, reg: usize, value: u8) {
        let palette = reg / 2;
        let base = (reg % 2) * 2;
        // Colour 0 of palettes 4-7 and 12-15 is non-writable — it is the
        // transparency colour and always reads 0 (ares guards this exactly as
        // `(address & 0x9) != 0x8`).
        if !(base == 0 && palette & 0x04 != 0) {
            self.palettes[palette][base] = value & 0x07;
        }
        self.palettes[palette][base + 1] = (value >> 4) & 0x07;
    }

    /// Resolve the final 4-bit shade for `palette` (0..=15), `colour` (0..=3)
    /// through the pool indirection.
    #[must_use]
    pub const fn shade(&self, palette: usize, colour: usize) -> u8 {
        self.pool[self.palettes[palette][colour] as usize]
    }

    /// The pool index a palette colour selects (before the pool lookup).
    #[must_use]
    pub const fn pool_index(&self, palette: usize, colour: usize) -> u8 {
        self.palettes[palette][colour]
    }
}

impl Default for MonoPalettes {
    fn default() -> Self {
        Self::new()
    }
}

/// Tile colour depth (`DISP_MODE` bit 7), which governs the colour-zero
/// transparency rule — *independently* of whether the display is mono or colour
/// (`DISP_MODE` bit 6). A 2bpp **colour** background still uses the 2bpp rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Depth {
    /// 2 bits per pixel (4 colours per tile) — "standard palette" mode.
    TwoBpp,
    /// 4 bits per pixel (16 colours per tile) — "sixteen colour" mode.
    FourBpp,
}

/// Whether colour index 0 is transparent for `palette` at pixel `depth`.
///
/// **Community bug #6:** transparency depends on the tile colour **depth**, not
/// on mono-vs-colour. Forcing index 0 transparent for every colour-mode palette
/// mis-renders WSC backgrounds drawn in 2bpp colour mode. Rules (WSdev Display;
/// ares `ppu` `opaque()` is `depth == 2 && !palette.bit(2)`):
/// - **2bpp:** palettes 0–3 and 8–11 are opaque (index 0 *not* transparent);
///   palettes 4–7 and 12–15 use index 0 as transparency — in both mono *and*
///   colour displays.
/// - **4bpp (16-colour):** every palette treats index 0 as transparent.
///
/// `REG_BACK_COLOR` is the always-drawn fallback and shows index 0 opaque.
#[must_use]
pub const fn color_zero_transparent(depth: Depth, palette: u8, as_back_color: bool) -> bool {
    if as_back_color {
        return false; // REG_BACK_COLOR displays index 0 opaque
    }
    match depth {
        // Palettes 4–7 and 12–15 (bit 2 set) use index 0 as transparency.
        Depth::TwoBpp => palette & 0x04 != 0,
        Depth::FourBpp => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Community bug #5: shades resolve through the pool, not a direct map.
    #[test]
    fn shade_resolves_through_the_pool() {
        let mut pal = MonoPalettes::new();
        pal.write_pool(0, 0x30); // pool[0]=0, pool[1]=3
        pal.write_pool(1, 0xF7); // pool[2]=7, pool[3]=0x0F (darkest)
        pal.write_palette(0, 0x03); // palette 0: colour0=idx3, colour1=idx0
        assert_eq!(pal.shade(0, 0), 0x0F, "colour0 -> pool[3] = darkest");
        assert_eq!(pal.shade(0, 1), 0x00, "colour1 -> pool[0] = brightest");
    }

    /// The essence of the indirection: changing the pool re-shades every palette
    /// that references it, without touching a single palette register.
    #[test]
    fn changing_the_pool_reshades_palettes_that_use_it() {
        let mut pal = MonoPalettes::new();
        pal.write_palette(0, 0x00); // palette 0, colour 0 -> pool index 0
        pal.write_pool(0, 0x05);
        assert_eq!(pal.shade(0, 0), 5);
        pal.write_pool(0, 0x0A);
        assert_eq!(
            pal.shade(0, 0),
            0x0A,
            "pool edit reshades via the indirection"
        );
    }

    #[test]
    fn only_three_index_bits_are_used_per_colour() {
        let mut pal = MonoPalettes::new();
        pal.write_palette(0, 0xFF); // both colours = 0x7 after masking to 3 bits
        assert_eq!(pal.pool_index(0, 0), 7);
        assert_eq!(pal.pool_index(0, 1), 7);
    }

    /// Community bug #5b: colour 0 of palettes 4-7 / 12-15 is non-writable.
    #[test]
    fn color_zero_is_write_protected_for_transparent_palettes() {
        let mut pal = MonoPalettes::new();
        // palette 5 (bit 2 set): reg 10 is its low byte (colours 0 and 1).
        pal.write_palette(10, 0x35); // would set colour0=5, colour1=3
        assert_eq!(
            pal.pool_index(5, 0),
            0,
            "colour 0 stays 0 (write-protected)"
        );
        assert_eq!(pal.pool_index(5, 1), 3, "colour 1 still writes");
        // palette 0 (bit 2 clear): colour 0 IS writable.
        pal.write_palette(0, 0x05);
        assert_eq!(pal.pool_index(0, 0), 5);
    }

    /// Community bug #6: at 2bpp, colour-zero transparency depends on the palette
    /// number — in mono *and* colour displays (the case the old mono/colour axis
    /// got wrong for 2bpp colour backgrounds).
    #[test]
    fn two_bpp_color_zero_by_palette_number() {
        for p in [0u8, 1, 2, 3, 8, 9, 10, 11] {
            assert!(
                !color_zero_transparent(Depth::TwoBpp, p, false),
                "2bpp palette {p}: index 0 opaque"
            );
        }
        for p in [4u8, 5, 6, 7, 12, 13, 14, 15] {
            assert!(
                color_zero_transparent(Depth::TwoBpp, p, false),
                "2bpp palette {p}: index 0 transparent"
            );
        }
    }

    /// Community bug #6: at 4bpp (16-colour) every palette's index 0 is
    /// transparent, except the always-drawn `REG_BACK_COLOR`.
    #[test]
    fn four_bpp_all_transparent_except_back_color() {
        for p in 0..16u8 {
            assert!(
                color_zero_transparent(Depth::FourBpp, p, false),
                "4bpp palette {p}: index 0 transparent"
            );
        }
        assert!(
            !color_zero_transparent(Depth::FourBpp, 0, true),
            "back colour opaque"
        );
        assert!(
            !color_zero_transparent(Depth::TwoBpp, 5, true),
            "back colour opaque at 2bpp too"
        );
    }
}
