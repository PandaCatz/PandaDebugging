// SPDX-License-Identifier: GPL-3.0-or-later
//! V30MZ arithmetic/logic unit with 8086/80186 flag semantics.
//!
//! Each operation updates the [`Flags`] it is given and returns the result.
//! Flag rules follow the documented 8086 model (see
//! `docs/hardware/01-cpu-v30mz.md` §flags): add/sub set CF/AF/OF/SF/ZF/PF; the
//! logical ops clear CF/OF/AF and set SF/ZF/PF; `INC`/`DEC` preserve CF; `NEG`
//! is `0 - operand`; `NOT` touches no flags.
//!
//! Post-division flag state and a handful of adjust-instruction edge cases are
//! still open questions (see the spec appendix) and are intentionally not
//! implemented here yet.

use crate::registers::Flags;

/// Operand width descriptor shared by the 8- and 16-bit paths.
#[derive(Clone, Copy)]
struct Width {
    bits: u32,
}

impl Width {
    const B8: Width = Width { bits: 8 };
    const B16: Width = Width { bits: 16 };

    const fn mask(self) -> u32 {
        (1u32 << self.bits) - 1
    }
    const fn sign(self) -> u32 {
        1u32 << (self.bits - 1)
    }
    const fn carry(self) -> u32 {
        1u32 << self.bits
    }
}

fn set_szp(flags: &mut Flags, result: u32, width: Width) {
    let r = result & width.mask();
    flags.zero = r == 0;
    flags.sign = (r & width.sign()) != 0;
    flags.parity = (r as u8).count_ones().is_multiple_of(2);
}

fn add_core(flags: &mut Flags, a: u32, b: u32, carry_in: u32, width: Width) -> u32 {
    let sum = a + b + carry_in;
    let result = sum & width.mask();
    flags.carry = (sum & width.carry()) != 0;
    flags.aux_carry = ((a ^ b ^ sum) & 0x10) != 0;
    flags.overflow = ((!(a ^ b)) & (a ^ result) & width.sign()) != 0;
    set_szp(flags, result, width);
    result
}

fn sub_core(flags: &mut Flags, a: u32, b: u32, borrow_in: u32, width: Width) -> u32 {
    let full = a as i32 - b as i32 - borrow_in as i32;
    let raw = full as u32;
    let result = raw & width.mask();
    flags.carry = full < 0;
    flags.aux_carry = ((a ^ b ^ raw) & 0x10) != 0;
    flags.overflow = ((a ^ b) & (a ^ result) & width.sign()) != 0;
    set_szp(flags, result, width);
    result
}

fn logic_flags(flags: &mut Flags, result: u32, width: Width) {
    flags.carry = false;
    flags.overflow = false;
    flags.aux_carry = false;
    set_szp(flags, result, width);
}

macro_rules! width_ops {
    ($t:ty, $w:expr, $add:ident, $adc:ident, $sub:ident, $sbb:ident, $cmp:ident,
     $and:ident, $or:ident, $xor:ident, $test:ident, $inc:ident, $dec:ident,
     $neg:ident, $not:ident) => {
        pub fn $add(flags: &mut Flags, a: $t, b: $t) -> $t {
            add_core(flags, a as u32, b as u32, 0, $w) as $t
        }
        pub fn $adc(flags: &mut Flags, a: $t, b: $t) -> $t {
            add_core(flags, a as u32, b as u32, flags.carry as u32, $w) as $t
        }
        pub fn $sub(flags: &mut Flags, a: $t, b: $t) -> $t {
            sub_core(flags, a as u32, b as u32, 0, $w) as $t
        }
        pub fn $sbb(flags: &mut Flags, a: $t, b: $t) -> $t {
            sub_core(flags, a as u32, b as u32, flags.carry as u32, $w) as $t
        }
        /// Compare (`a - b`), updating flags only.
        pub fn $cmp(flags: &mut Flags, a: $t, b: $t) {
            sub_core(flags, a as u32, b as u32, 0, $w);
        }
        pub fn $and(flags: &mut Flags, a: $t, b: $t) -> $t {
            let r = a & b;
            logic_flags(flags, r as u32, $w);
            r
        }
        pub fn $or(flags: &mut Flags, a: $t, b: $t) -> $t {
            let r = a | b;
            logic_flags(flags, r as u32, $w);
            r
        }
        pub fn $xor(flags: &mut Flags, a: $t, b: $t) -> $t {
            let r = a ^ b;
            logic_flags(flags, r as u32, $w);
            r
        }
        /// Test (`a & b`), updating flags only.
        pub fn $test(flags: &mut Flags, a: $t, b: $t) {
            logic_flags(flags, (a & b) as u32, $w);
        }
        /// Increment, preserving CF (per hardware).
        pub fn $inc(flags: &mut Flags, a: $t) -> $t {
            let saved_carry = flags.carry;
            let r = add_core(flags, a as u32, 1, 0, $w) as $t;
            flags.carry = saved_carry;
            r
        }
        /// Decrement, preserving CF (per hardware).
        pub fn $dec(flags: &mut Flags, a: $t) -> $t {
            let saved_carry = flags.carry;
            let r = sub_core(flags, a as u32, 1, 0, $w) as $t;
            flags.carry = saved_carry;
            r
        }
        pub fn $neg(flags: &mut Flags, a: $t) -> $t {
            sub_core(flags, 0, a as u32, 0, $w) as $t
        }
        /// Bitwise complement; affects no flags.
        pub fn $not(a: $t) -> $t {
            !a
        }
    };
}

width_ops!(
    u8,
    Width::B8,
    add8,
    adc8,
    sub8,
    sbb8,
    cmp8,
    and8,
    or8,
    xor8,
    test8,
    inc8,
    dec8,
    neg8,
    not8
);
width_ops!(
    u16,
    Width::B16,
    add16,
    adc16,
    sub16,
    sbb16,
    cmp16,
    and16,
    or16,
    xor16,
    test16,
    inc16,
    dec16,
    neg16,
    not16
);

/// GRP2 shift/rotate. `op` is the ModR/M reg field:
/// 0=ROL 1=ROR 2=RCL 3=RCR 4=SHL/SAL 5=SHR 6=SAL (undocumented, = SHL) 7=SAR.
///
/// The `count` is used **raw**. Whether the V30MZ masks the count to 5 bits
/// (80186+ behaviour) or uses the full 8-bit count (8086/V20 behaviour) is
/// unresolved (see `docs/hardware/01-cpu-v30mz.md`) and must be checked against
/// WSCpuTest before it is trusted.
///
/// Rotates affect only CF and OF; shifts also set SF/ZF/PF (AF undefined). OF is
/// defined only for a count of 1. A count of 0 changes nothing.
fn shift_rotate(flags: &mut Flags, op: u8, value: u16, count: u8, word: bool) -> u16 {
    let width = if word { Width::B16 } else { Width::B8 };
    let bits = width.bits;
    let mask = width.mask();
    let mut val = u32::from(value) & mask;
    if count == 0 {
        return val as u16;
    }
    let n = u32::from(count);
    let mut cf = u32::from(flags.carry);
    let top = |v: u32| (v >> (bits - 1)) & 1;
    match op {
        0 => {
            // ROL
            for _ in 0..n {
                let hi = top(val);
                val = ((val << 1) | hi) & mask;
                cf = hi;
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = (top(val) ^ cf) != 0;
            }
        }
        1 => {
            // ROR
            for _ in 0..n {
                let lo = val & 1;
                val = (val >> 1) | (lo << (bits - 1));
                cf = lo;
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = (top(val) ^ ((val >> (bits - 2)) & 1)) != 0;
            }
        }
        2 => {
            // RCL (through carry)
            for _ in 0..n {
                let hi = top(val);
                val = ((val << 1) | cf) & mask;
                cf = hi;
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = (top(val) ^ cf) != 0;
            }
        }
        3 => {
            // RCR (through carry)
            for _ in 0..n {
                let lo = val & 1;
                val = (val >> 1) | (cf << (bits - 1));
                cf = lo;
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = (top(val) ^ ((val >> (bits - 2)) & 1)) != 0;
            }
        }
        4 | 6 => {
            // SHL / SAL
            for _ in 0..n {
                cf = top(val);
                val = (val << 1) & mask;
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = (top(val) ^ cf) != 0;
            }
            set_szp(flags, val, width);
        }
        5 => {
            // SHR
            let orig_top = top(val);
            for _ in 0..n {
                cf = val & 1;
                val >>= 1;
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = orig_top != 0;
            }
            set_szp(flags, val, width);
        }
        _ => {
            // SAR (7): arithmetic, keeps the sign bit
            let sign = top(val);
            for _ in 0..n {
                cf = val & 1;
                val = (val >> 1) | (sign << (bits - 1));
            }
            flags.carry = cf != 0;
            if n == 1 {
                flags.overflow = false;
            }
            set_szp(flags, val, width);
        }
    }
    (val & mask) as u16
}

pub fn shift_rotate8(flags: &mut Flags, op: u8, value: u8, count: u8) -> u8 {
    shift_rotate(flags, op, u16::from(value), count, false) as u8
}

pub fn shift_rotate16(flags: &mut Flags, op: u8, value: u16, count: u8) -> u16 {
    shift_rotate(flags, op, value, count, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> Flags {
        Flags::default()
    }

    #[test]
    fn add8_carry_and_aux_and_zero() {
        let mut fl = f();
        assert_eq!(add8(&mut fl, 0xFF, 0x01), 0x00);
        assert!(fl.carry && fl.zero && fl.aux_carry);
        assert!(!fl.overflow && !fl.sign);
        assert!(fl.parity);
    }

    #[test]
    fn add8_signed_overflow() {
        let mut fl = f();
        assert_eq!(add8(&mut fl, 0x7F, 0x01), 0x80);
        assert!(fl.overflow && fl.sign && fl.aux_carry && !fl.carry);
    }

    #[test]
    fn adc8_uses_carry_in() {
        let mut fl = f();
        fl.carry = true;
        assert_eq!(adc8(&mut fl, 0x0F, 0x00), 0x10);
        assert!(fl.aux_carry && !fl.carry);
    }

    #[test]
    fn sub8_borrow_and_overflow() {
        let mut fl = f();
        assert_eq!(sub8(&mut fl, 0x00, 0x01), 0xFF);
        assert!(fl.carry && fl.sign && fl.aux_carry && !fl.overflow);

        let mut fl = f();
        assert_eq!(sub8(&mut fl, 0x80, 0x01), 0x7F);
        assert!(fl.overflow && !fl.carry && !fl.sign);
    }

    #[test]
    fn cmp8_sets_flags_like_sub() {
        let mut fl = f();
        cmp8(&mut fl, 5, 5);
        assert!(fl.zero && !fl.carry);
    }

    #[test]
    fn logical_ops_clear_carry_overflow_aux() {
        let mut fl = f();
        fl.carry = true;
        fl.overflow = true;
        fl.aux_carry = true;
        assert_eq!(and8(&mut fl, 0x0F, 0xF0), 0x00);
        assert!(!fl.carry && !fl.overflow && !fl.aux_carry && fl.zero && fl.parity);
    }

    #[test]
    fn inc_dec_preserve_carry() {
        let mut fl = f();
        fl.carry = true;
        assert_eq!(inc8(&mut fl, 0x0F), 0x10);
        assert!(fl.carry, "INC preserves CF");
        assert!(fl.aux_carry);
        assert_eq!(dec8(&mut fl, 0x00), 0xFF);
        assert!(fl.carry, "DEC preserves CF");
    }

    #[test]
    fn neg8_edges() {
        let mut fl = f();
        assert_eq!(neg8(&mut fl, 0x01), 0xFF);
        assert!(fl.carry);
        let mut fl = f();
        assert_eq!(neg8(&mut fl, 0x00), 0x00);
        assert!(!fl.carry && fl.zero);
        let mut fl = f();
        assert_eq!(neg8(&mut fl, 0x80), 0x80);
        assert!(fl.overflow && fl.carry);
    }

    #[test]
    fn width16_add_and_sub() {
        let mut fl = f();
        assert_eq!(add16(&mut fl, 0xFFFF, 0x0001), 0x0000);
        assert!(fl.carry && fl.zero);
        let mut fl = f();
        assert_eq!(sub16(&mut fl, 0x0000, 0x0001), 0xFFFF);
        assert!(fl.carry && fl.sign);
    }

    #[test]
    fn not_affects_no_flags() {
        assert_eq!(not8(0x0F), 0xF0);
        assert_eq!(not16(0x00FF), 0xFF00);
    }

    #[test]
    fn shl8_carry_overflow_zero() {
        let mut fl = f();
        assert_eq!(shift_rotate8(&mut fl, 4, 0x80, 1), 0x00);
        assert!(fl.carry && fl.zero);
        let mut fl = f();
        assert_eq!(shift_rotate8(&mut fl, 4, 0x40, 1), 0x80);
        assert!(!fl.carry && fl.overflow && fl.sign);
    }

    #[test]
    fn rcl8_rotates_through_carry() {
        let mut fl = f();
        fl.carry = true;
        assert_eq!(shift_rotate8(&mut fl, 2, 0x00, 1), 0x01);
        assert!(!fl.carry);
    }

    #[test]
    fn rotates_leave_zero_flag_untouched() {
        let mut fl = f();
        fl.zero = true;
        assert_eq!(shift_rotate8(&mut fl, 0, 0x01, 1), 0x02); // ROL
        assert!(fl.zero, "rotates must not alter SF/ZF/PF");
    }

    #[test]
    fn shift_count_zero_is_a_noop() {
        let mut fl = f();
        fl.carry = true;
        assert_eq!(shift_rotate8(&mut fl, 4, 0x80, 0), 0x80);
        assert!(fl.carry, "count 0 leaves flags untouched");
    }

    #[test]
    fn sar_keeps_sign_bit() {
        let mut fl = f();
        assert_eq!(shift_rotate8(&mut fl, 7, 0x80, 1), 0xC0);
    }
}
