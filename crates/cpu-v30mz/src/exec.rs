//! Instruction execution: `step()` fetches, decodes, and executes one
//! instruction. This is the growing opcode table; each increment adds a
//! coherent block with tests. **No cycle counting yet** — the master-vs-CPU
//! cycle-unit question (see `docs/hardware/01-cpu-v30mz.md`) is unresolved.
//!
//! Implemented so far: segment-override / LOCK prefixes; the ALU block
//! (`ADD/OR/ADC/SBB/AND/SUB/XOR/CMP`, `0x00–0x3D`, all six forms) and GRP1
//! immediate ALU (`0x80–0x83`); MOV (all forms, segment regs, `LEA`, moffs,
//! imm); `XCHG`; `INC`/`DEC` r16; `TEST`; `CBW`/`CWD`; `SALC`; the stack ops
//! (`PUSH`/`POP`, `PUSHF`/`POPF`); control flow (`Jcc`, `JMP`, `CALL`, `RET`,
//! `LOOP`); and the flag / `NOP` / `HLT` opcodes.
//!
//! Not yet: GRP2 shifts/rotates, GRP3 (`MUL`/`DIV`/`NOT`/`NEG`), GRP4/5
//! (`INC`/`DEC`/indirect `CALL`/`JMP`/`PUSH` r/m), string ops + `REP`, `INT`/
//! `IRET` and interrupt delivery, and `IN`/`OUT`.

use crate::Cpu;
use crate::alu;
use crate::bus::CpuBus;
use crate::decode::Rm;
use crate::registers::physical_address;

/// Outcome of a single [`Cpu::step`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    /// One instruction executed.
    Executed,
    /// The CPU is halted (`HLT`); it stays halted until an interrupt resumes it.
    Halted,
    /// The opcode is not implemented yet. The opcode byte has been consumed.
    Unimplemented(u8),
}

impl Cpu {
    /// Fetch and execute one instruction (after any prefixes).
    pub fn step(&mut self, bus: &mut dyn CpuBus) -> Step {
        if self.halted {
            return Step::Halted;
        }
        let mut segment_override: Option<u16> = None;
        loop {
            let opcode = self.fetch_u8(bus);
            match opcode {
                0x26 => segment_override = Some(self.regs.es),
                0x2E => segment_override = Some(self.regs.cs),
                0x36 => segment_override = Some(self.regs.ss),
                0x3E => segment_override = Some(self.regs.ds),
                0xF0 => {} // LOCK: no observable effect on WonderSwan
                _ => return self.execute(bus, opcode, segment_override),
            }
        }
    }

    fn execute(&mut self, bus: &mut dyn CpuBus, opcode: u8, seg: Option<u16>) -> Step {
        // ALU block: opcodes 0x00-0x3D where the low 3 bits select the operand
        // form (0..5). reg field of (opcode>>3)&7 selects the operation.
        if opcode < 0x40 && (opcode & 0x07) < 6 {
            self.execute_alu(bus, opcode, seg);
            return Step::Executed;
        }

        match opcode {
            // ---- MOV, register / memory ----
            0x88 => {
                let m = self.decode_modrm(bus, seg);
                let v = self.regs.reg8(m.reg);
                self.write_rm8(bus, m.rm, v);
            }
            0x89 => {
                let m = self.decode_modrm(bus, seg);
                let v = self.regs.reg16(m.reg);
                self.write_rm16(bus, m.rm, v);
            }
            0x8A => {
                let m = self.decode_modrm(bus, seg);
                let v = self.read_rm8(bus, m.rm);
                self.regs.set_reg8(m.reg, v);
            }
            0x8B => {
                let m = self.decode_modrm(bus, seg);
                let v = self.read_rm16(bus, m.rm);
                self.regs.set_reg16(m.reg, v);
            }
            0x8C => {
                // MOV Ew, Sw
                let m = self.decode_modrm(bus, seg);
                let v = self.regs.seg(m.reg & 3);
                self.write_rm16(bus, m.rm, v);
            }
            0x8E => {
                // MOV Sw, Ew
                let m = self.decode_modrm(bus, seg);
                let v = self.read_rm16(bus, m.rm);
                self.regs.set_seg(m.reg & 3, v);
            }
            0x8D => {
                // LEA Gv, M — load the effective offset (register form is undefined)
                let m = self.decode_modrm(bus, seg);
                if let Rm::Memory { offset, .. } = m.rm {
                    self.regs.set_reg16(m.reg, offset);
                }
            }
            0xC6 => {
                let m = self.decode_modrm(bus, seg);
                let imm = self.fetch_u8(bus);
                self.write_rm8(bus, m.rm, imm);
            }
            0xC7 => {
                let m = self.decode_modrm(bus, seg);
                let imm = self.fetch_u16(bus);
                self.write_rm16(bus, m.rm, imm);
            }
            // ---- MOV, accumulator <-> direct memory offset ----
            0xA0 => {
                let off = self.fetch_u16(bus);
                let s = seg.unwrap_or(self.regs.ds);
                let v = bus.read_u8(physical_address(s, off));
                self.regs.set_al(v);
            }
            0xA1 => {
                let off = self.fetch_u16(bus);
                let s = seg.unwrap_or(self.regs.ds);
                let v = self.read_mem16(bus, s, off);
                self.regs.ax = v;
            }
            0xA2 => {
                let off = self.fetch_u16(bus);
                let s = seg.unwrap_or(self.regs.ds);
                let al = self.regs.al();
                bus.write_u8(physical_address(s, off), al);
            }
            0xA3 => {
                let off = self.fetch_u16(bus);
                let s = seg.unwrap_or(self.regs.ds);
                let ax = self.regs.ax;
                self.write_mem16(bus, s, off, ax);
            }
            // ---- MOV reg, immediate ----
            0xB0..=0xB7 => {
                let imm = self.fetch_u8(bus);
                self.regs.set_reg8(opcode & 7, imm);
            }
            0xB8..=0xBF => {
                let imm = self.fetch_u16(bus);
                self.regs.set_reg16(opcode & 7, imm);
            }
            // ---- XCHG ----
            0x86 => {
                let m = self.decode_modrm(bus, seg);
                let a = self.read_rm8(bus, m.rm);
                let b = self.regs.reg8(m.reg);
                self.write_rm8(bus, m.rm, b);
                self.regs.set_reg8(m.reg, a);
            }
            0x87 => {
                let m = self.decode_modrm(bus, seg);
                let a = self.read_rm16(bus, m.rm);
                let b = self.regs.reg16(m.reg);
                self.write_rm16(bus, m.rm, b);
                self.regs.set_reg16(m.reg, a);
            }
            0x90..=0x97 => {
                // XCHG AX, r16  (0x90 = XCHG AX,AX = NOP)
                let i = opcode & 7;
                let ax = self.regs.ax;
                let r = self.regs.reg16(i);
                self.regs.ax = r;
                self.regs.set_reg16(i, ax);
            }
            // ---- INC/DEC r16, immediate ALU, TEST, CBW/CWD, SALC ----
            0x40..=0x47 => {
                let i = opcode & 7;
                let v0 = self.regs.reg16(i);
                let v = alu::inc16(&mut self.regs.flags, v0);
                self.regs.set_reg16(i, v);
            }
            0x48..=0x4F => {
                let i = opcode & 7;
                let v0 = self.regs.reg16(i);
                let v = alu::dec16(&mut self.regs.flags, v0);
                self.regs.set_reg16(i, v);
            }
            0x80 | 0x82 => {
                // GRP1 Eb, Ib
                let m = self.decode_modrm(bus, seg);
                let imm = self.fetch_u8(bus);
                let a = self.read_rm8(bus, m.rm);
                if let Some(r) = self.apply_alu8(m.reg, a, imm) {
                    self.write_rm8(bus, m.rm, r);
                }
            }
            0x81 => {
                // GRP1 Ev, Iv
                let m = self.decode_modrm(bus, seg);
                let imm = self.fetch_u16(bus);
                let a = self.read_rm16(bus, m.rm);
                if let Some(r) = self.apply_alu16(m.reg, a, imm) {
                    self.write_rm16(bus, m.rm, r);
                }
            }
            0x83 => {
                // GRP1 Ev, Ib (sign-extended immediate)
                let m = self.decode_modrm(bus, seg);
                let imm = self.fetch_u8(bus) as i8 as i16 as u16;
                let a = self.read_rm16(bus, m.rm);
                if let Some(r) = self.apply_alu16(m.reg, a, imm) {
                    self.write_rm16(bus, m.rm, r);
                }
            }
            0x84 => {
                // TEST Eb, Gb
                let m = self.decode_modrm(bus, seg);
                let a = self.read_rm8(bus, m.rm);
                let b = self.regs.reg8(m.reg);
                alu::test8(&mut self.regs.flags, a, b);
            }
            0x85 => {
                // TEST Ev, Gv
                let m = self.decode_modrm(bus, seg);
                let a = self.read_rm16(bus, m.rm);
                let b = self.regs.reg16(m.reg);
                alu::test16(&mut self.regs.flags, a, b);
            }
            0xA8 => {
                let imm = self.fetch_u8(bus);
                let a = self.regs.al();
                alu::test8(&mut self.regs.flags, a, imm);
            }
            0xA9 => {
                let imm = self.fetch_u16(bus);
                let a = self.regs.ax;
                alu::test16(&mut self.regs.flags, a, imm);
            }
            0x98 => {
                // CBW: sign-extend AL into AX
                let al = self.regs.al();
                self.regs.ax = al as i8 as i16 as u16;
            }
            0x99 => {
                // CWD: sign-extend AX into DX
                self.regs.dx = if self.regs.ax & 0x8000 != 0 {
                    0xFFFF
                } else {
                    0
                };
            }
            0xD6 => {
                // SALC (undocumented on V30MZ): AL = CF ? 0xFF : 0x00
                let v = if self.regs.flags.carry { 0xFF } else { 0x00 };
                self.regs.set_al(v);
            }
            // ---- stack ----
            0x50..=0x57 => {
                let v = self.regs.reg16(opcode & 7);
                self.push16(bus, v);
            }
            0x58..=0x5F => {
                let v = self.pop16(bus);
                self.regs.set_reg16(opcode & 7, v);
            }
            0x06 | 0x0E | 0x16 | 0x1E => {
                // PUSH ES/CS/SS/DS
                let v = self.regs.seg((opcode >> 3) & 3);
                self.push16(bus, v);
            }
            0x07 | 0x17 | 0x1F => {
                // POP ES/SS/DS  (0x0F is a NOP on V30MZ, not POP CS)
                let v = self.pop16(bus);
                self.regs.set_seg((opcode >> 3) & 3, v);
            }
            0x8F => {
                // POP Ev
                let m = self.decode_modrm(bus, seg);
                let v = self.pop16(bus);
                self.write_rm16(bus, m.rm, v);
            }
            0x9C => {
                // PUSHF
                let v = self.regs.flags.to_word();
                self.push16(bus, v);
            }
            0x9D => {
                // POPF
                let v = self.pop16(bus);
                self.regs.flags = crate::registers::Flags::from_word(v);
            }
            // ---- control flow ----
            0x70..=0x7F => {
                // Jcc rel8
                let rel = self.fetch_u8(bus) as i8 as i16 as u16;
                if self.condition(opcode & 0x0F) {
                    self.regs.ip = self.regs.ip.wrapping_add(rel);
                }
            }
            0xEB => {
                let rel = self.fetch_u8(bus) as i8 as i16 as u16;
                self.regs.ip = self.regs.ip.wrapping_add(rel);
            }
            0xE9 => {
                let rel = self.fetch_u16(bus);
                self.regs.ip = self.regs.ip.wrapping_add(rel);
            }
            0xEA => {
                // JMP far ptr16:16
                let ip = self.fetch_u16(bus);
                let cs = self.fetch_u16(bus);
                self.regs.ip = ip;
                self.regs.cs = cs;
            }
            0xE8 => {
                // CALL rel16
                let rel = self.fetch_u16(bus);
                let ret = self.regs.ip;
                self.push16(bus, ret);
                self.regs.ip = self.regs.ip.wrapping_add(rel);
            }
            0x9A => {
                // CALL far ptr16:16
                let new_ip = self.fetch_u16(bus);
                let new_cs = self.fetch_u16(bus);
                let (cs, ip) = (self.regs.cs, self.regs.ip);
                self.push16(bus, cs);
                self.push16(bus, ip);
                self.regs.cs = new_cs;
                self.regs.ip = new_ip;
            }
            0xC3 => self.regs.ip = self.pop16(bus), // RET near
            0xC2 => {
                // RET near, pop imm16 extra
                let imm = self.fetch_u16(bus);
                self.regs.ip = self.pop16(bus);
                self.regs.sp = self.regs.sp.wrapping_add(imm);
            }
            0xCB => {
                // RETF
                self.regs.ip = self.pop16(bus);
                self.regs.cs = self.pop16(bus);
            }
            0xCA => {
                // RETF, pop imm16 extra
                let imm = self.fetch_u16(bus);
                self.regs.ip = self.pop16(bus);
                self.regs.cs = self.pop16(bus);
                self.regs.sp = self.regs.sp.wrapping_add(imm);
            }
            0xE0..=0xE3 => {
                // LOOPNE / LOOPE / LOOP / JCXZ
                let rel = self.fetch_u8(bus) as i8 as i16 as u16;
                let take = match opcode {
                    0xE0 => {
                        self.regs.cx = self.regs.cx.wrapping_sub(1);
                        self.regs.cx != 0 && !self.regs.flags.zero
                    }
                    0xE1 => {
                        self.regs.cx = self.regs.cx.wrapping_sub(1);
                        self.regs.cx != 0 && self.regs.flags.zero
                    }
                    0xE2 => {
                        self.regs.cx = self.regs.cx.wrapping_sub(1);
                        self.regs.cx != 0
                    }
                    _ => self.regs.cx == 0, // 0xE3 JCXZ (no decrement)
                };
                if take {
                    self.regs.ip = self.regs.ip.wrapping_add(rel);
                }
            }
            // ---- flags / control ----
            0xF4 => {
                self.halted = true;
                return Step::Halted;
            }
            0xF5 => self.regs.flags.carry = !self.regs.flags.carry, // CMC
            0xF8 => self.regs.flags.carry = false,                  // CLC
            0xF9 => self.regs.flags.carry = true,                   // STC
            0xFA => self.regs.flags.interrupt = false,              // CLI
            0xFB => self.regs.flags.interrupt = true,               // STI
            0xFC => self.regs.flags.direction = false,              // CLD
            0xFD => self.regs.flags.direction = true,               // STD
            _ => return Step::Unimplemented(opcode),
        }
        Step::Executed
    }

    fn execute_alu(&mut self, bus: &mut dyn CpuBus, opcode: u8, seg: Option<u16>) {
        let operation = (opcode >> 3) & 7;
        match opcode & 7 {
            0 => {
                // Eb, Gb  (dest = r/m, src = reg)
                let m = self.decode_modrm(bus, seg);
                let a = self.read_rm8(bus, m.rm);
                let b = self.regs.reg8(m.reg);
                if let Some(r) = self.apply_alu8(operation, a, b) {
                    self.write_rm8(bus, m.rm, r);
                }
            }
            1 => {
                // Ev, Gv
                let m = self.decode_modrm(bus, seg);
                let a = self.read_rm16(bus, m.rm);
                let b = self.regs.reg16(m.reg);
                if let Some(r) = self.apply_alu16(operation, a, b) {
                    self.write_rm16(bus, m.rm, r);
                }
            }
            2 => {
                // Gb, Eb  (dest = reg, src = r/m)
                let m = self.decode_modrm(bus, seg);
                let a = self.regs.reg8(m.reg);
                let b = self.read_rm8(bus, m.rm);
                if let Some(r) = self.apply_alu8(operation, a, b) {
                    self.regs.set_reg8(m.reg, r);
                }
            }
            3 => {
                // Gv, Ev
                let m = self.decode_modrm(bus, seg);
                let a = self.regs.reg16(m.reg);
                let b = self.read_rm16(bus, m.rm);
                if let Some(r) = self.apply_alu16(operation, a, b) {
                    self.regs.set_reg16(m.reg, r);
                }
            }
            4 => {
                // AL, Ib
                let a = self.regs.al();
                let b = self.fetch_u8(bus);
                if let Some(r) = self.apply_alu8(operation, a, b) {
                    self.regs.set_al(r);
                }
            }
            _ => {
                // AX, Iv  (opcode & 7 == 5)
                let a = self.regs.ax;
                let b = self.fetch_u16(bus);
                if let Some(r) = self.apply_alu16(operation, a, b) {
                    self.regs.ax = r;
                }
            }
        }
    }

    /// Run one 8-bit ALU operation; `None` means "flags only" (`CMP`).
    fn apply_alu8(&mut self, operation: u8, a: u8, b: u8) -> Option<u8> {
        let f = &mut self.regs.flags;
        Some(match operation {
            0 => alu::add8(f, a, b),
            1 => alu::or8(f, a, b),
            2 => alu::adc8(f, a, b),
            3 => alu::sbb8(f, a, b),
            4 => alu::and8(f, a, b),
            5 => alu::sub8(f, a, b),
            6 => alu::xor8(f, a, b),
            _ => {
                alu::cmp8(f, a, b);
                return None;
            }
        })
    }

    fn apply_alu16(&mut self, operation: u8, a: u16, b: u16) -> Option<u16> {
        let f = &mut self.regs.flags;
        Some(match operation {
            0 => alu::add16(f, a, b),
            1 => alu::or16(f, a, b),
            2 => alu::adc16(f, a, b),
            3 => alu::sbb16(f, a, b),
            4 => alu::and16(f, a, b),
            5 => alu::sub16(f, a, b),
            6 => alu::xor16(f, a, b),
            _ => {
                alu::cmp16(f, a, b);
                return None;
            }
        })
    }

    /// Evaluate an 8086 condition code (the low nibble of a `Jcc` opcode).
    fn condition(&self, cc: u8) -> bool {
        let f = &self.regs.flags;
        match cc {
            0x0 => f.overflow,
            0x1 => !f.overflow,
            0x2 => f.carry,
            0x3 => !f.carry,
            0x4 => f.zero,
            0x5 => !f.zero,
            0x6 => f.carry || f.zero,
            0x7 => !f.carry && !f.zero,
            0x8 => f.sign,
            0x9 => !f.sign,
            0xA => f.parity,
            0xB => !f.parity,
            0xC => f.sign != f.overflow,
            0xD => f.sign == f.overflow,
            0xE => f.zero || (f.sign != f.overflow),
            _ => !f.zero && (f.sign == f.overflow),
        }
    }

    fn read_rm8(&mut self, bus: &mut dyn CpuBus, rm: Rm) -> u8 {
        match rm {
            Rm::Register(index) => self.regs.reg8(index),
            Rm::Memory { address, .. } => bus.read_u8(address),
        }
    }

    fn write_rm8(&mut self, bus: &mut dyn CpuBus, rm: Rm, value: u8) {
        match rm {
            Rm::Register(index) => self.regs.set_reg8(index, value),
            Rm::Memory { address, .. } => bus.write_u8(address, value),
        }
    }

    fn read_rm16(&mut self, bus: &mut dyn CpuBus, rm: Rm) -> u16 {
        match rm {
            Rm::Register(index) => self.regs.reg16(index),
            Rm::Memory {
                segment, offset, ..
            } => self.read_mem16(bus, segment, offset),
        }
    }

    fn write_rm16(&mut self, bus: &mut dyn CpuBus, rm: Rm, value: u16) {
        match rm {
            Rm::Register(index) => self.regs.set_reg16(index, value),
            Rm::Memory {
                segment, offset, ..
            } => self.write_mem16(bus, segment, offset, value),
        }
    }

    /// Read a little-endian word at `segment:offset`, wrapping the offset within
    /// the 16-bit segment.
    fn read_mem16(&mut self, bus: &mut dyn CpuBus, segment: u16, offset: u16) -> u16 {
        let lo = bus.read_u8(physical_address(segment, offset));
        let hi = bus.read_u8(physical_address(segment, offset.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    /// Write a little-endian word at `segment:offset`, wrapping the offset.
    fn write_mem16(&mut self, bus: &mut dyn CpuBus, segment: u16, offset: u16, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        bus.write_u8(physical_address(segment, offset), lo);
        bus.write_u8(physical_address(segment, offset.wrapping_add(1)), hi);
    }

    /// Push a word: predecrement `SP` by 2, then store at `SS:SP`.
    fn push16(&mut self, bus: &mut dyn CpuBus, value: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(2);
        let (segment, offset) = (self.regs.ss, self.regs.sp);
        self.write_mem16(bus, segment, offset, value);
    }

    /// Pop a word: load from `SS:SP`, then postincrement `SP` by 2.
    fn pop16(&mut self, bus: &mut dyn CpuBus) -> u16 {
        let (segment, offset) = (self.regs.ss, self.regs.sp);
        let value = self.read_mem16(bus, segment, offset);
        self.regs.sp = self.regs.sp.wrapping_add(2);
        value
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
        fn load(&mut self, addr: u32, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.mem[(addr as usize) + i] = *b;
            }
        }
    }
    impl CpuBus for TestBus {
        fn read_u8(&mut self, a: u32) -> u8 {
            self.mem[(a & ADDR_MASK) as usize]
        }
        fn write_u8(&mut self, a: u32, v: u8) {
            self.mem[(a & ADDR_MASK) as usize] = v;
        }
        fn io_read_u8(&mut self, _p: u16) -> u8 {
            0
        }
        fn io_write_u8(&mut self, _p: u16, _v: u8) {}
    }

    /// Fresh CPU executing from 0000:0000 for compact test programs.
    fn cpu() -> Cpu {
        let mut c = Cpu::new();
        c.regs.cs = 0;
        c.regs.ip = 0;
        c
    }

    #[test]
    fn add_al_imm8() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x04, 0x05]); // ADD AL, 5
        let mut cpu = cpu();
        cpu.regs.set_al(0x10);
        assert_eq!(cpu.step(&mut bus), Step::Executed);
        assert_eq!(cpu.regs.al(), 0x15);
        assert_eq!(cpu.regs.ip, 2);
    }

    #[test]
    fn add_ax_imm16_sets_carry_and_zero() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x05, 0x01, 0x00]); // ADD AX, 1
        let mut cpu = cpu();
        cpu.regs.ax = 0xFFFF;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0x0000);
        assert!(cpu.regs.flags.carry && cpu.regs.flags.zero);
    }

    #[test]
    fn add_mem_reg_writes_memory() {
        let mut bus = TestBus::new();
        // ADD [BX], CL  = 0x00 /rm; modrm mod=00 reg=1(CL) rm=7(BX)
        bus.load(0, &[0x00, 0b00_001_111]);
        let mut cpu = cpu();
        cpu.regs.ds = 0x2000;
        cpu.regs.bx = 0x0100;
        cpu.regs.set_cl(0x22);
        let addr = physical_address(0x2000, 0x0100);
        bus.write_u8(addr, 0x11);
        cpu.step(&mut bus);
        assert_eq!(bus.read_u8(addr), 0x33);
    }

    #[test]
    fn sub_reg_reg_via_gv_ev() {
        let mut bus = TestBus::new();
        // SUB AX, CX = 0x2B /r (Gv,Ev), modrm mod=11 reg=0(AX) rm=1(CX)
        bus.load(0, &[0x2B, 0b11_000_001]);
        let mut cpu = cpu();
        cpu.regs.ax = 0x0005;
        cpu.regs.cx = 0x0003;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0x0002);
        assert!(!cpu.regs.flags.carry);
    }

    #[test]
    fn cmp_ax_imm_updates_flags_only() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x3D, 0x05, 0x00]); // CMP AX, 5
        let mut cpu = cpu();
        cpu.regs.ax = 0x0005;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0x0005, "CMP does not write back");
        assert!(cpu.regs.flags.zero);
    }

    #[test]
    fn segment_override_prefix_redirects_memory() {
        let mut bus = TestBus::new();
        // ES: ADD [BX], AL -> 0x26 0x00 modrm(reg=0 AL, rm=7 BX)
        bus.load(0, &[0x26, 0x00, 0b00_000_111]);
        let mut cpu = cpu();
        cpu.regs.ds = 0x1000;
        cpu.regs.es = 0x4000;
        cpu.regs.bx = 0x0002;
        cpu.regs.set_al(0x01);
        let es_addr = physical_address(0x4000, 0x0002);
        bus.write_u8(es_addr, 0x10);
        cpu.step(&mut bus);
        assert_eq!(bus.read_u8(es_addr), 0x11, "wrote via ES, not DS");
    }

    #[test]
    fn flag_opcodes() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF9, 0xF5, 0xF8, 0xFB]); // STC, CMC, CLC, STI
        let mut cpu = cpu();
        cpu.step(&mut bus);
        assert!(cpu.regs.flags.carry, "STC");
        cpu.step(&mut bus);
        assert!(!cpu.regs.flags.carry, "CMC toggles set->clear");
        cpu.step(&mut bus);
        assert!(!cpu.regs.flags.carry, "CLC");
        cpu.step(&mut bus);
        assert!(cpu.regs.flags.interrupt, "STI");
    }

    #[test]
    fn nop_advances_and_hlt_halts() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x90, 0xF4]); // NOP, HLT
        let mut cpu = cpu();
        assert_eq!(cpu.step(&mut bus), Step::Executed);
        assert_eq!(cpu.regs.ip, 1);
        assert_eq!(cpu.step(&mut bus), Step::Halted);
        assert!(cpu.halted);
        assert_eq!(cpu.step(&mut bus), Step::Halted, "stays halted");
    }

    #[test]
    fn unimplemented_opcode_is_reported() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x0F]); // single-byte NOP on V30MZ, but not decoded yet
        let mut cpu = cpu();
        assert_eq!(cpu.step(&mut bus), Step::Unimplemented(0x0F));
    }

    #[test]
    fn mov_reg_immediate() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xB8, 0x34, 0x12, 0xB1, 0x55]); // MOV AX,0x1234 ; MOV CL,0x55
        let mut cpu = cpu();
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0x1234);
        assert_eq!(cpu.regs.cl(), 0x55);
    }

    #[test]
    fn mov_memory_word_roundtrip() {
        let mut bus = TestBus::new();
        // MOV [BX],AX (89, mod=00 reg=0 rm=7) ; MOV DX,[BX] (8B, mod=00 reg=2 rm=7)
        bus.load(0, &[0x89, 0b00_000_111, 0x8B, 0b00_010_111]);
        let mut cpu = cpu();
        cpu.regs.ds = 0x2000;
        cpu.regs.bx = 0x0040;
        cpu.regs.ax = 0xBEEF;
        cpu.step(&mut bus);
        let addr = physical_address(0x2000, 0x0040);
        assert_eq!(bus.read_u8(addr), 0xEF);
        assert_eq!(bus.read_u8(addr + 1), 0xBE);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.dx, 0xBEEF);
    }

    #[test]
    fn mov_segment_registers() {
        let mut bus = TestBus::new();
        // MOV ES,AX (8E reg=0=ES rm=0=AX) ; MOV BX,ES (8C reg=0=ES rm=3=BX)
        bus.load(0, &[0x8E, 0b11_000_000, 0x8C, 0b11_000_011]);
        let mut cpu = cpu();
        cpu.regs.ax = 0x9000;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.es, 0x9000);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.bx, 0x9000);
    }

    #[test]
    fn mov_accumulator_direct_offset() {
        let mut bus = TestBus::new();
        // MOV AL,[0x0050] (A0) ; MOV [0x0060],AL (A2)
        bus.load(0, &[0xA0, 0x50, 0x00, 0xA2, 0x60, 0x00]);
        let mut cpu = cpu();
        cpu.regs.ds = 0x1000;
        bus.write_u8(physical_address(0x1000, 0x0050), 0x7E);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x7E);
        cpu.step(&mut bus);
        assert_eq!(bus.read_u8(physical_address(0x1000, 0x0060)), 0x7E);
    }

    #[test]
    fn lea_loads_effective_offset() {
        let mut bus = TestBus::new();
        // LEA BX,[BX+SI+0x10]  (8D, mod=01 reg=3=BX rm=0=BX+SI, disp8=0x10)
        bus.load(0, &[0x8D, 0b01_011_000, 0x10]);
        let mut cpu = cpu();
        cpu.regs.bx = 0x0100;
        cpu.regs.si = 0x0002;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.bx, 0x0112);
    }

    #[test]
    fn xchg_swaps_operands() {
        let mut bus = TestBus::new();
        // XCHG CX,DX (87, mod=11 reg=1=CX rm=2=DX) ; XCHG AX,BX (0x93)
        bus.load(0, &[0x87, 0b11_001_010, 0x93]);
        let mut cpu = cpu();
        cpu.regs.cx = 0x1111;
        cpu.regs.dx = 0x2222;
        cpu.regs.ax = 0xAAAA;
        cpu.regs.bx = 0xBBBB;
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.cx, cpu.regs.dx), (0x2222, 0x1111));
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.ax, cpu.regs.bx), (0xBBBB, 0xAAAA));
    }

    #[test]
    fn push_pop_register_roundtrip() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x50, 0x5B]); // PUSH AX ; POP BX
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.regs.ax = 0xCAFE;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.sp, 0x00FE, "SP predecremented by 2");
        assert_eq!(bus.read_u8(physical_address(0x3000, 0x00FE)), 0xFE);
        assert_eq!(bus.read_u8(physical_address(0x3000, 0x00FF)), 0xCA);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.bx, 0xCAFE);
        assert_eq!(cpu.regs.sp, 0x0100, "SP restored");
    }

    #[test]
    fn pushf_popf_roundtrip() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x9C, 0x9D]); // PUSHF ; POPF
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0010;
        cpu.regs.flags.carry = true;
        cpu.regs.flags.zero = true;
        cpu.step(&mut bus); // PUSHF
        cpu.regs.flags.carry = false;
        cpu.regs.flags.zero = false;
        cpu.step(&mut bus); // POPF restores
        assert!(cpu.regs.flags.carry && cpu.regs.flags.zero);
    }

    #[test]
    fn push_pop_segment_registers() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x06, 0x1F]); // PUSH ES ; POP DS
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0020;
        cpu.regs.es = 0x7777;
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ds, 0x7777);
    }

    #[test]
    fn pop_into_memory_operand() {
        let mut bus = TestBus::new();
        // PUSH AX ; POP [BX]  (8F, mod=00 reg=0 rm=7)
        bus.load(0, &[0x50, 0x8F, 0b00_000_111]);
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0040;
        cpu.regs.ds = 0x2000;
        cpu.regs.bx = 0x0080;
        cpu.regs.ax = 0x1234;
        cpu.step(&mut bus); // PUSH AX
        cpu.step(&mut bus); // POP [BX]
        let addr = physical_address(0x2000, 0x0080);
        assert_eq!(bus.read_u8(addr), 0x34);
        assert_eq!(bus.read_u8(addr + 1), 0x12);
        assert_eq!(cpu.regs.sp, 0x0040);
    }

    #[test]
    fn jmp_rel8() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xEB, 0x03, 0x00, 0x00, 0x00, 0xF9]); // JMP +3 ; ... ; STC
        let mut cpu = cpu();
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ip, 0x05);
        cpu.step(&mut bus);
        assert!(cpu.regs.flags.carry);
    }

    #[test]
    fn jz_taken_and_not_taken() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x74, 0x10]); // JZ +0x10
        let mut taken = cpu();
        taken.regs.flags.zero = true;
        taken.step(&mut bus);
        assert_eq!(taken.regs.ip, 0x12);
        let mut fallthrough = cpu();
        fallthrough.regs.flags.zero = false;
        fallthrough.step(&mut bus);
        assert_eq!(fallthrough.regs.ip, 0x02);
    }

    #[test]
    fn jl_uses_sign_ne_overflow() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x7C, 0x08]); // JL +8
        let mut cpu = cpu();
        cpu.regs.flags.sign = true; // SF != OF -> less
        cpu.regs.flags.overflow = false;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ip, 0x0A);
    }

    #[test]
    fn call_and_ret_near() {
        let mut bus = TestBus::new();
        // CALL +2 at 0 (return addr 3, target 5); RET at 5
        bus.load(0, &[0xE8, 0x02, 0x00, 0x00, 0x00, 0xC3]);
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ip, 0x05);
        assert_eq!(cpu.regs.sp, 0x00FE);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ip, 0x03, "returned to instruction after CALL");
        assert_eq!(cpu.regs.sp, 0x0100);
    }

    #[test]
    fn loop_decrements_cx_and_branches() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xE2, 0xFE]); // LOOP -2 (to self)
        let mut cpu = cpu();
        cpu.regs.cx = 2;
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.cx, cpu.regs.ip), (1, 0));
        cpu.regs.ip = 0;
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.cx, cpu.regs.ip), (0, 2), "CX hit 0: not taken");
    }

    #[test]
    fn jcxz_branches_only_when_cx_zero() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xE3, 0x05]); // JCXZ +5
        let mut zero = cpu();
        zero.regs.cx = 0;
        zero.step(&mut bus);
        assert_eq!(zero.regs.ip, 0x07);
        let mut nonzero = cpu();
        nonzero.regs.cx = 1;
        nonzero.step(&mut bus);
        assert_eq!(nonzero.regs.ip, 0x02);
    }

    #[test]
    fn far_call_and_retf() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x9A, 0x00, 0x00, 0x00, 0x20]); // CALL FAR 0x2000:0x0000
        bus.write_u8(physical_address(0x2000, 0x0000), 0xCB); // RETF at target
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.cs, cpu.regs.ip), (0x2000, 0x0000));
        assert_eq!(cpu.regs.sp, 0x00FC, "pushed CS and IP (4 bytes)");
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.cs, cpu.regs.ip), (0x0000, 0x0005));
        assert_eq!(cpu.regs.sp, 0x0100);
    }

    #[test]
    fn inc_dec_r16() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x40, 0x49]); // INC AX ; DEC CX
        let mut cpu = cpu();
        cpu.regs.ax = 0x00FF;
        cpu.regs.cx = 0x0001;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0x0100);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.cx, 0x0000);
        assert!(cpu.regs.flags.zero);
    }

    #[test]
    fn grp1_add_and_sign_extended_immediate() {
        let mut bus = TestBus::new();
        // ADD BX,0x05 then ADD BX,-1 (both via 83 /0, mod=11 rm=3=BX)
        bus.load(0, &[0x83, 0b11_000_011, 0x05, 0x83, 0b11_000_011, 0xFF]);
        let mut cpu = cpu();
        cpu.regs.bx = 0x0010;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.bx, 0x0015);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.bx, 0x0014, "0xFF sign-extended to -1");
    }

    #[test]
    fn grp1_cmp_immediate_is_flags_only() {
        let mut bus = TestBus::new();
        // CMP AL,0x10 via 80 /7 (reg=7=CMP, mod=11 rm=0=AL)
        bus.load(0, &[0x80, 0b11_111_000, 0x10]);
        let mut cpu = cpu();
        cpu.regs.set_al(0x10);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x10, "CMP does not write back");
        assert!(cpu.regs.flags.zero);
    }

    #[test]
    fn test_accumulator_immediate() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xA8, 0x0F]); // TEST AL,0x0F
        let mut cpu = cpu();
        cpu.regs.set_al(0xF0);
        cpu.step(&mut bus);
        assert!(cpu.regs.flags.zero, "0xF0 & 0x0F == 0");
    }

    #[test]
    fn cbw_and_cwd_sign_extend() {
        let mut bus = TestBus::new();
        bus.load(0, &[0x98, 0x99]); // CBW ; CWD
        let mut cpu = cpu();
        cpu.regs.set_al(0x80);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0xFF80);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.dx, 0xFFFF);
    }

    #[test]
    fn salc_reflects_carry() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xD6, 0xD6]); // SALC ; SALC
        let mut cpu = cpu();
        cpu.regs.flags.carry = true;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0xFF);
        cpu.regs.flags.carry = false;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x00);
    }
}
