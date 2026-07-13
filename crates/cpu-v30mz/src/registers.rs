//! V30MZ register file, flags, and 20-bit segmented addressing.
//!
//! All facts here are spec-grounded and timing-independent (see
//! `docs/hardware/01-cpu-v30mz.md`). The reset vector `CS:IP = FFFF:0000` is
//! confirmed (WSMan over HTTP + WSdev Boot ROM). The exact FLAGS word read back
//! immediately after reset (whether the mode bit MD reads 0 or 1 → `0x7002` vs
//! `0xF002`) is an open question; we therefore reset the *defined* flags to a
//! known state and do not materialise a disputed MD bit.

/// Compute a 20-bit physical address from a segment:offset pair, with the
/// classic 8086/V30MZ wrap at the 1 MiB boundary.
#[must_use]
pub const fn physical_address(segment: u16, offset: u16) -> u32 {
    (((segment as u32) << 4).wrapping_add(offset as u32)) & 0x000F_FFFF
}

/// The processor status flags. Defined bits only; the reserved/mode bits are
/// handled explicitly in [`Flags::to_word`] / [`Flags::from_word`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Flags {
    pub carry: bool,     // CF, bit 0
    pub parity: bool,    // PF, bit 2
    pub aux_carry: bool, // AF, bit 4
    pub zero: bool,      // ZF, bit 6
    pub sign: bool,      // SF, bit 7
    pub trap: bool,      // TF, bit 8
    pub interrupt: bool, // IF, bit 9
    pub direction: bool, // DF, bit 10
    pub overflow: bool,  // OF, bit 11
}

impl Flags {
    const CF: u16 = 1 << 0;
    const RESERVED1: u16 = 1 << 1; // reads as 1 on 8086-class parts
    const PF: u16 = 1 << 2;
    const AF: u16 = 1 << 4;
    const ZF: u16 = 1 << 6;
    const SF: u16 = 1 << 7;
    const TF: u16 = 1 << 8;
    const IF: u16 = 1 << 9;
    const DF: u16 = 1 << 10;
    const OF: u16 = 1 << 11;

    /// Materialise the defined flags plus the always-1 reserved bit 1.
    ///
    /// Bits 12–15 (including the V30MZ mode bit MD at 15) are left 0 here: their
    /// power-on read-back value is unresolved (see module docs) and must not be
    /// baked into a literal until confirmed against hardware / WSCpuTest.
    #[must_use]
    pub const fn to_word(self) -> u16 {
        let mut w = Self::RESERVED1;
        if self.carry {
            w |= Self::CF;
        }
        if self.parity {
            w |= Self::PF;
        }
        if self.aux_carry {
            w |= Self::AF;
        }
        if self.zero {
            w |= Self::ZF;
        }
        if self.sign {
            w |= Self::SF;
        }
        if self.trap {
            w |= Self::TF;
        }
        if self.interrupt {
            w |= Self::IF;
        }
        if self.direction {
            w |= Self::DF;
        }
        if self.overflow {
            w |= Self::OF;
        }
        w
    }

    /// Decode the defined flags from a FLAGS word (reserved/mode bits ignored).
    #[must_use]
    pub const fn from_word(word: u16) -> Self {
        Self {
            carry: word & Self::CF != 0,
            parity: word & Self::PF != 0,
            aux_carry: word & Self::AF != 0,
            zero: word & Self::ZF != 0,
            sign: word & Self::SF != 0,
            trap: word & Self::TF != 0,
            interrupt: word & Self::IF != 0,
            direction: word & Self::DF != 0,
            overflow: word & Self::OF != 0,
        }
    }
}

/// The V30MZ register file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Registers {
    pub ax: u16,
    pub cx: u16,
    pub dx: u16,
    pub bx: u16,
    pub sp: u16,
    pub bp: u16,
    pub si: u16,
    pub di: u16,
    pub es: u16,
    pub cs: u16,
    pub ss: u16,
    pub ds: u16,
    pub ip: u16,
    pub flags: Flags,
}

macro_rules! byte_accessors {
    ($word:ident, $lo_get:ident, $lo_set:ident, $hi_get:ident, $hi_set:ident) => {
        #[must_use]
        pub const fn $lo_get(&self) -> u8 {
            self.$word as u8
        }
        pub const fn $lo_set(&mut self, value: u8) {
            self.$word = (self.$word & 0xFF00) | (value as u16);
        }
        #[must_use]
        pub const fn $hi_get(&self) -> u8 {
            (self.$word >> 8) as u8
        }
        pub const fn $hi_set(&mut self, value: u8) {
            self.$word = (self.$word & 0x00FF) | ((value as u16) << 8);
        }
    };
}

impl Registers {
    /// Power-on / hard-reset state. `CS:IP = FFFF:0000`; other segments and the
    /// general registers are zero; defined flags are cleared.
    #[must_use]
    pub const fn reset() -> Self {
        Self {
            ax: 0,
            cx: 0,
            dx: 0,
            bx: 0,
            sp: 0,
            bp: 0,
            si: 0,
            di: 0,
            es: 0,
            cs: 0xFFFF,
            ss: 0,
            ds: 0,
            ip: 0,
            flags: Flags {
                carry: false,
                parity: false,
                aux_carry: false,
                zero: false,
                sign: false,
                trap: false,
                interrupt: false,
                direction: false,
                overflow: false,
            },
        }
    }

    byte_accessors!(ax, al, set_al, ah, set_ah);
    byte_accessors!(cx, cl, set_cl, ch, set_ch);
    byte_accessors!(dx, dl, set_dl, dh, set_dh);
    byte_accessors!(bx, bl, set_bl, bh, set_bh);

    /// Physical address of the next instruction byte (`CS:IP`).
    #[must_use]
    pub const fn code_address(&self) -> u32 {
        physical_address(self.cs, self.ip)
    }

    /// Physical address of the current stack top (`SS:SP`).
    #[must_use]
    pub const fn stack_address(&self) -> u32 {
        physical_address(self.ss, self.sp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_vector_is_ffff_0000() {
        let r = Registers::reset();
        assert_eq!(r.cs, 0xFFFF);
        assert_eq!(r.ip, 0x0000);
        assert_eq!(r.code_address(), 0xFFFF0);
        assert_eq!((r.ds, r.es, r.ss, r.sp), (0, 0, 0, 0));
    }

    #[test]
    fn physical_address_wraps_at_one_mib() {
        // 0xFFFF:0xFFFF -> 0xFFFF0 + 0xFFFF = 0x10FFEF, wrapped to 0x0FFEF.
        assert_eq!(physical_address(0xFFFF, 0xFFFF), 0x0FFEF);
        assert_eq!(physical_address(0x1000, 0x0000), 0x10000);
        assert_eq!(physical_address(0x0000, 0x0000), 0x00000);
    }

    #[test]
    fn byte_accessors_split_word_registers() {
        let mut r = Registers::reset();
        r.ax = 0x1234;
        assert_eq!(r.al(), 0x34);
        assert_eq!(r.ah(), 0x12);
        r.set_al(0xEF);
        r.set_ah(0xBE);
        assert_eq!(r.ax, 0xBEEF);
    }

    #[test]
    fn flags_word_roundtrips_defined_bits() {
        let f = Flags {
            carry: true,
            zero: true,
            interrupt: true,
            overflow: true,
            ..Flags::default()
        };
        let w = f.to_word();
        assert_eq!(w & (1 << 1), 1 << 1, "reserved bit 1 always set");
        assert_eq!(Flags::from_word(w), f);
    }

    #[test]
    fn flags_ignore_reserved_and_mode_bits_on_decode() {
        // Setting reserved/mode bits (incl. MD at 15) must not invent flags.
        assert_eq!(Flags::from_word(0xF002), Flags::default());
        assert_eq!(Flags::from_word(0x7002), Flags::default());
    }
}
