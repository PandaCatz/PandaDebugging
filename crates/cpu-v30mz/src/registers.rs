//! V30MZ register file, flags, and 20-bit segmented addressing.
//!
//! All facts here are spec-grounded and timing-independent (see
//! `docs/hardware/01-cpu-v30mz.md`). The reset vector `CS:IP = FFFF:0000` is
//! confirmed (WSMan over HTTP + WSdev Boot ROM). The materialised FLAGS word
//! sets bits 1 and 12–15 to 1 (`0xF002`) in native mode — the value pushed by
//! `PUSHF`/interrupt entry — confirmed against the V20 single-step oracle and
//! ARMV30MZ. (WSCpuTest can still verify the MD bit for the V30MZ specifically.)

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
    const PF: u16 = 1 << 2;
    const AF: u16 = 1 << 4;
    const ZF: u16 = 1 << 6;
    const SF: u16 = 1 << 7;
    const TF: u16 = 1 << 8;
    const IF: u16 = 1 << 9;
    const DF: u16 = 1 << 10;
    const OF: u16 = 1 << 11;
    /// Bits that always read 1 on 8086/V20/V30MZ in native mode: reserved bit 1
    /// plus bits 12–15 (which include the V30MZ mode bit MD at 15). The V20
    /// single-step oracle and ARMV30MZ both push `0xF002`, so this is now
    /// resolved for native-mode operation.
    const ALWAYS_ONE: u16 = 0xF002;

    /// Materialise the defined flags plus the always-1 bits ([`Flags::ALWAYS_ONE`]).
    /// This is exactly the word `PUSHF` and interrupt entry write to the stack.
    #[must_use]
    pub const fn to_word(self) -> u16 {
        let mut w = Self::ALWAYS_ONE;
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

    /// Read a 16-bit register by its 3-bit encoding
    /// (0=AX 1=CX 2=DX 3=BX 4=SP 5=BP 6=SI 7=DI).
    #[must_use]
    pub const fn reg16(&self, index: u8) -> u16 {
        match index & 7 {
            0 => self.ax,
            1 => self.cx,
            2 => self.dx,
            3 => self.bx,
            4 => self.sp,
            5 => self.bp,
            6 => self.si,
            _ => self.di,
        }
    }

    pub const fn set_reg16(&mut self, index: u8, value: u16) {
        match index & 7 {
            0 => self.ax = value,
            1 => self.cx = value,
            2 => self.dx = value,
            3 => self.bx = value,
            4 => self.sp = value,
            5 => self.bp = value,
            6 => self.si = value,
            _ => self.di = value,
        }
    }

    /// Read an 8-bit register by its 3-bit encoding
    /// (0=AL 1=CL 2=DL 3=BL 4=AH 5=CH 6=DH 7=BH).
    #[must_use]
    pub const fn reg8(&self, index: u8) -> u8 {
        match index & 7 {
            0 => self.al(),
            1 => self.cl(),
            2 => self.dl(),
            3 => self.bl(),
            4 => self.ah(),
            5 => self.ch(),
            6 => self.dh(),
            _ => self.bh(),
        }
    }

    pub const fn set_reg8(&mut self, index: u8, value: u8) {
        match index & 7 {
            0 => self.set_al(value),
            1 => self.set_cl(value),
            2 => self.set_dl(value),
            3 => self.set_bl(value),
            4 => self.set_ah(value),
            5 => self.set_ch(value),
            6 => self.set_dh(value),
            _ => self.set_bh(value),
        }
    }

    /// Read a segment register by its 2-bit encoding (0=ES 1=CS 2=SS 3=DS).
    #[must_use]
    pub const fn seg(&self, index: u8) -> u16 {
        match index & 3 {
            0 => self.es,
            1 => self.cs,
            2 => self.ss,
            _ => self.ds,
        }
    }

    pub const fn set_seg(&mut self, index: u8, value: u16) {
        match index & 3 {
            0 => self.es = value,
            1 => self.cs = value,
            2 => self.ss = value,
            _ => self.ds = value,
        }
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

    #[test]
    fn register_index_accessors_match_named_fields() {
        let mut r = Registers::reset();
        r.ax = 0x1122;
        r.cx = 0x3344;
        r.sp = 0x5566;
        r.di = 0x7788;
        assert_eq!(r.reg16(0), 0x1122, "AX");
        assert_eq!(r.reg16(4), 0x5566, "SP");
        assert_eq!(r.reg16(7), 0x7788, "DI");
        assert_eq!(r.reg8(0), 0x22, "AL");
        assert_eq!(r.reg8(4), 0x11, "AH");
        assert_eq!(r.reg8(1), 0x44, "CL");
        r.set_reg16(5, 0x9999); // BP
        assert_eq!(r.bp, 0x9999);
        r.set_reg8(4, 0xEE); // AH
        assert_eq!(r.ah(), 0xEE);
        assert_eq!(r.seg(1), 0xFFFF, "CS after reset");
        r.set_seg(0, 0xABCD); // ES
        assert_eq!(r.es, 0xABCD);
    }
}
