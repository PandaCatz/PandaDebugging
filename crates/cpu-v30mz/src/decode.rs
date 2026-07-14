// SPDX-License-Identifier: GPL-3.0-or-later
//! ModR/M byte decoding and effective-address computation.
//!
//! Covers the 16-bit 8086/V30MZ addressing modes. Segment defaults follow the
//! rule "any mode with `BP` as a base defaults to `SS`, otherwise `DS`"; a
//! segment-override prefix replaces that default. Offset arithmetic wraps within
//! the 16-bit segment; the physical address is formed by [`physical_address`].

use crate::Cpu;
use crate::bus::CpuBus;
use crate::registers::physical_address;

/// The r/m operand of a ModR/M byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rm {
    /// `mod == 11`: a register, by its 3-bit encoding.
    Register(u8),
    /// A memory operand with its resolved physical address plus the
    /// segment/offset it came from (kept for word-wrap and debugging).
    Memory {
        address: u32,
        segment: u16,
        offset: u16,
    },
}

/// A decoded ModR/M byte: the `reg` field and the r/m operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModRm {
    pub reg: u8,
    pub rm: Rm,
}

impl Cpu {
    /// Fetch a ModR/M byte at `CS:IP` (plus any displacement) and resolve its
    /// operands. `segment_override`, if present, is the segment *value* that
    /// replaces the addressing mode's default segment.
    pub fn decode_modrm(&mut self, bus: &mut dyn CpuBus, segment_override: Option<u16>) -> ModRm {
        let modrm = self.fetch_u8(bus);
        let mode = modrm >> 6;
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;

        if mode == 0b11 {
            return ModRm {
                reg,
                rm: Rm::Register(rm),
            };
        }

        let (base, base_is_ss) = match rm {
            0 => (self.regs.bx.wrapping_add(self.regs.si), false),
            1 => (self.regs.bx.wrapping_add(self.regs.di), false),
            2 => (self.regs.bp.wrapping_add(self.regs.si), true),
            3 => (self.regs.bp.wrapping_add(self.regs.di), true),
            4 => (self.regs.si, false),
            5 => (self.regs.di, false),
            6 if mode == 0 => (0, false), // disp16 direct address
            6 => (self.regs.bp, true),
            _ => (self.regs.bx, false),
        };

        let displacement = match mode {
            0 if rm == 6 => self.fetch_u16(bus),
            0 => 0,
            1 => self.fetch_u8(bus) as i8 as i16 as u16, // sign-extended disp8
            _ => self.fetch_u16(bus),
        };

        let offset = base.wrapping_add(displacement);
        let default_segment = if base_is_ss {
            self.regs.ss
        } else {
            self.regs.ds
        };
        let segment = segment_override.unwrap_or(default_segment);
        ModRm {
            reg,
            rm: Rm::Memory {
                address: physical_address(segment, offset),
                segment,
                offset,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ADDR_MASK;

    struct TestBus {
        mem: Vec<u8>,
    }
    impl TestBus {
        fn new() -> Self {
            Self {
                mem: vec![0; (ADDR_MASK as usize) + 1],
            }
        }
    }
    impl CpuBus for TestBus {
        fn read_u8(&mut self, address: u32) -> u8 {
            self.mem[(address & ADDR_MASK) as usize]
        }
        fn write_u8(&mut self, address: u32, value: u8) {
            self.mem[(address & ADDR_MASK) as usize] = value;
        }
        fn io_read_u8(&mut self, _port: u16) -> u8 {
            0
        }
        fn io_write_u8(&mut self, _port: u16, _value: u8) {}
    }

    fn cpu_at_origin() -> Cpu {
        let mut cpu = Cpu::new();
        cpu.regs.cs = 0;
        cpu.regs.ip = 0;
        cpu
    }

    #[test]
    fn register_form_reads_reg_and_rm_indices() {
        let mut bus = TestBus::new();
        let mut cpu = cpu_at_origin();
        bus.write_u8(0, 0b11_010_001); // mod=11 reg=2 rm=1
        let m = cpu.decode_modrm(&mut bus, None);
        assert_eq!(m.reg, 2);
        assert_eq!(m.rm, Rm::Register(1));
        assert_eq!(cpu.regs.ip, 1);
    }

    #[test]
    fn bx_si_mode0_uses_ds() {
        let mut bus = TestBus::new();
        let mut cpu = cpu_at_origin();
        cpu.regs.bx = 0x0100;
        cpu.regs.si = 0x0020;
        cpu.regs.ds = 0x2000;
        bus.write_u8(0, 0b00_000_000); // [BX+SI], DS
        let m = cpu.decode_modrm(&mut bus, None);
        assert_eq!(
            m.rm,
            Rm::Memory {
                address: physical_address(0x2000, 0x0120),
                segment: 0x2000,
                offset: 0x0120,
            }
        );
    }

    #[test]
    fn bp_base_defaults_to_ss_with_signed_disp8() {
        let mut bus = TestBus::new();
        let mut cpu = cpu_at_origin();
        cpu.regs.bp = 0x0010;
        cpu.regs.si = 0x0002;
        cpu.regs.ss = 0x3000;
        bus.write_u8(0, 0b01_000_010); // [BP+SI]+disp8, SS
        bus.write_u8(1, 0xFF); // disp8 = -1
        let m = cpu.decode_modrm(&mut bus, None);
        match m.rm {
            Rm::Memory {
                segment, offset, ..
            } => {
                assert_eq!(segment, 0x3000);
                assert_eq!(offset, 0x0011); // 0x10 + 0x02 - 1
            }
            Rm::Register(_) => panic!("expected memory operand"),
        }
        assert_eq!(cpu.regs.ip, 2);
    }

    #[test]
    fn mod0_rm6_is_direct_disp16() {
        let mut bus = TestBus::new();
        let mut cpu = cpu_at_origin();
        cpu.regs.ds = 0x1000;
        bus.write_u8(0, 0b00_001_110); // direct disp16
        bus.write_u8(1, 0x34);
        bus.write_u8(2, 0x12);
        let m = cpu.decode_modrm(&mut bus, None);
        match m.rm {
            Rm::Memory {
                segment, offset, ..
            } => {
                assert_eq!((segment, offset), (0x1000, 0x1234));
            }
            Rm::Register(_) => panic!("expected memory operand"),
        }
        assert_eq!(cpu.regs.ip, 3);
    }

    #[test]
    fn segment_override_replaces_default() {
        let mut bus = TestBus::new();
        let mut cpu = cpu_at_origin();
        cpu.regs.bx = 0x0100;
        cpu.regs.ds = 0x2000;
        let es = 0x9000;
        bus.write_u8(0, 0b00_000_111); // [BX], default DS
        let m = cpu.decode_modrm(&mut bus, Some(es));
        match m.rm {
            Rm::Memory { segment, .. } => assert_eq!(segment, es),
            Rm::Register(_) => panic!("expected memory operand"),
        }
    }
}
