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
        self.palettes[palette][base] = value & 0x07;
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

/// Display mode, which governs the colour-zero transparency rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayMode {
    /// Mono (ASWAN / WS-compatibility mode).
    Mono,
    /// Colour (WSC / SwanCrystal).
    Color,
}

/// Whether colour index 0 is transparent for `palette` under `mode`.
///
/// **Community bug #6 (deep-dive / WSMan):** forcing index 0 transparent for
/// every colour-mode palette renders certain WSC backgrounds wrong (ares fixed
/// this in v144). The correct rules:
/// - **Mono:** palettes 0–3 and 8–11 are opaque (index 0 is *not* transparent);
///   palettes 4–7 and 12–15 use index 0 as transparency.
/// - **Colour:** every palette treats index 0 as transparent — *except* when the
///   colour is drawn through `REG_BACK_COLOR`, which shows index 0 opaque.
///
/// The colour-0 palette entry is always writable/stored regardless (that is the
/// other half of the ares fix); this decides only whether it is *drawn*.
#[must_use]
pub const fn color_zero_transparent(mode: DisplayMode, palette: u8, as_back_color: bool) -> bool {
    if as_back_color {
        return false; // REG_BACK_COLOR displays index 0 opaque
    }
    match mode {
        // Palettes 4–7 and 12–15 (bit 2 set) use index 0 as transparency.
        DisplayMode::Mono => palette & 0x04 != 0,
        DisplayMode::Color => true,
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

    /// Community bug #6: mono colour-zero transparency depends on the palette.
    #[test]
    fn mono_color_zero_transparency_by_palette() {
        for p in [0u8, 1, 2, 3, 8, 9, 10, 11] {
            assert!(
                !color_zero_transparent(DisplayMode::Mono, p, false),
                "mono palette {p}: index 0 is opaque"
            );
        }
        for p in [4u8, 5, 6, 7, 12, 13, 14, 15] {
            assert!(
                color_zero_transparent(DisplayMode::Mono, p, false),
                "mono palette {p}: index 0 is transparent"
            );
        }
    }

    /// Community bug #6: colour mode is all-transparent except the back colour.
    #[test]
    fn color_mode_all_transparent_except_back_color() {
        for p in 0..16u8 {
            assert!(
                color_zero_transparent(DisplayMode::Color, p, false),
                "colour palette {p}: index 0 transparent"
            );
        }
        assert!(
            !color_zero_transparent(DisplayMode::Color, 0, true),
            "REG_BACK_COLOR shows index 0 opaque"
        );
        assert!(
            !color_zero_transparent(DisplayMode::Mono, 5, true),
            "back colour is opaque in mono too"
        );
    }
}
