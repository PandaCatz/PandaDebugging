// SPDX-License-Identifier: GPL-3.0-or-later
//! Instruction execution: `step()` fetches, decodes, and executes one
//! instruction. This is the growing opcode table; each increment adds a
//! coherent block with tests. **No cycle counting yet** — the master-vs-CPU
//! cycle-unit question (see `docs/hardware/01-cpu-v30mz.md`) is unresolved.
//!
//! Implemented so far: the documented 8086/80186 instruction set as used on the
//! V30MZ — ALU (+ GRP1/GRP3 groups), MOV/`XCHG`/`LEA`, `INC`/`DEC`, `TEST`,
//! `CBW`/`CWD`, `SALC`, `MUL`/`IMUL`/`DIV`/`IDIV`, GRP2 shifts/rotates, GRP4/5
//! (indirect `CALL`/`JMP`/`PUSH`), the stack ops, string ops + `REP`, control
//! flow, `IN`/`OUT`, `INT`/`INTO`/`IRET` with the interrupt-delivery sequence,
//! and the flag / `NOP` / `HLT` opcodes. Prefixes: segment override, `LOCK`,
//! `REP`/`REPE`/`REPNE`.
//!
//! Not yet: hardware IRQ delivery (the machine must consult
//! `core-ws::InterruptController` before each step) and **all cycle timing**
//! (blocked on the cycle-unit question). A few V30MZ-undocumented slots
//! (e.g. `0xF1`) still report `Step::Unimplemented`.

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
        let mut rep: Option<u8> = None;
        loop {
            let opcode = self.fetch_u8(bus);
            match opcode {
                0x26 => segment_override = Some(self.regs.es),
                0x2E => segment_override = Some(self.regs.cs),
                0x36 => segment_override = Some(self.regs.ss),
                0x3E => segment_override = Some(self.regs.ds),
                0xF0 => {}                         // LOCK: no observable effect
                0xF2 | 0xF3 => rep = Some(opcode), // REPNE / REP(E)
                _ => return self.execute(bus, opcode, segment_override, rep),
            }
        }
    }

    fn execute(
        &mut self,
        bus: &mut dyn CpuBus,
        opcode: u8,
        seg: Option<u16>,
        rep: Option<u8>,
    ) -> Step {
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
            // ---- string ops (honour an optional REP prefix) ----
            0xA4..=0xA7 | 0xAA..=0xAF => self.execute_string(bus, opcode, seg, rep),
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
            // ---- GRP2 shifts / rotates ----
            0xC0 | 0xC1 | 0xD0 | 0xD1 | 0xD2 | 0xD3 => self.execute_grp2(bus, opcode, seg),
            // ---- I/O ports ----
            0xE4 => {
                let port = u16::from(self.fetch_u8(bus));
                let v = bus.io_read_u8(port);
                self.regs.set_al(v);
            }
            0xE5 => {
                let port = u16::from(self.fetch_u8(bus));
                self.regs.ax = bus.io_read_u16(port);
            }
            0xE6 => {
                let port = u16::from(self.fetch_u8(bus));
                let al = self.regs.al();
                bus.io_write_u8(port, al);
            }
            0xE7 => {
                let port = u16::from(self.fetch_u8(bus));
                let ax = self.regs.ax;
                bus.io_write_u16(port, ax);
            }
            0xEC => {
                let v = bus.io_read_u8(self.regs.dx);
                self.regs.set_al(v);
            }
            0xED => self.regs.ax = bus.io_read_u16(self.regs.dx),
            0xEE => {
                let al = self.regs.al();
                bus.io_write_u8(self.regs.dx, al);
            }
            0xEF => {
                let ax = self.regs.ax;
                bus.io_write_u16(self.regs.dx, ax);
            }
            // ---- interrupts ----
            0xCC => self.service_interrupt(bus, 3), // INT3
            0xCD => {
                let vector = self.fetch_u8(bus);
                self.service_interrupt(bus, vector);
            }
            0xCE => {
                // INTO: trap on overflow
                if self.regs.flags.overflow {
                    self.service_interrupt(bus, 4);
                }
            }
            0xCF => {
                // IRET
                self.regs.ip = self.pop16(bus);
                self.regs.cs = self.pop16(bus);
                let flags = self.pop16(bus);
                self.regs.flags = crate::registers::Flags::from_word(flags);
            }
            // ---- group opcodes (GRP3 F6/F7, GRP4 FE, GRP5 FF) ----
            0xF6 | 0xF7 => self.execute_grp3(bus, opcode, seg),
            0xFE => {
                if !self.execute_grp4(bus, seg) {
                    return Step::Unimplemented(opcode);
                }
            }
            0xFF => {
                if !self.execute_grp5(bus, seg) {
                    return Step::Unimplemented(opcode);
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

    /// GRP3 (`0xF6`/`0xF7`): TEST/NOT/NEG/MUL/IMUL/DIV/IDIV.
    fn execute_grp3(&mut self, bus: &mut dyn CpuBus, opcode: u8, seg: Option<u16>) {
        let word = opcode & 1 == 1;
        let m = self.decode_modrm(bus, seg);
        match m.reg {
            0 | 1 => {
                // TEST E, imm
                if word {
                    let imm = self.fetch_u16(bus);
                    let a = self.read_rm16(bus, m.rm);
                    alu::test16(&mut self.regs.flags, a, imm);
                } else {
                    let imm = self.fetch_u8(bus);
                    let a = self.read_rm8(bus, m.rm);
                    alu::test8(&mut self.regs.flags, a, imm);
                }
            }
            2 => {
                // NOT (no flags)
                if word {
                    let v = alu::not16(self.read_rm16(bus, m.rm));
                    self.write_rm16(bus, m.rm, v);
                } else {
                    let v = alu::not8(self.read_rm8(bus, m.rm));
                    self.write_rm8(bus, m.rm, v);
                }
            }
            3 => {
                // NEG
                if word {
                    let a = self.read_rm16(bus, m.rm);
                    let v = alu::neg16(&mut self.regs.flags, a);
                    self.write_rm16(bus, m.rm, v);
                } else {
                    let a = self.read_rm8(bus, m.rm);
                    let v = alu::neg8(&mut self.regs.flags, a);
                    self.write_rm8(bus, m.rm, v);
                }
            }
            4 => {
                if word {
                    let src = self.read_rm16(bus, m.rm);
                    self.mul16(src);
                } else {
                    let src = self.read_rm8(bus, m.rm);
                    self.mul8(src);
                }
            }
            5 => {
                if word {
                    let src = self.read_rm16(bus, m.rm);
                    self.imul16(src);
                } else {
                    let src = self.read_rm8(bus, m.rm);
                    self.imul8(src);
                }
            }
            6 => {
                if word {
                    let src = self.read_rm16(bus, m.rm);
                    self.div16(bus, src);
                } else {
                    let src = self.read_rm8(bus, m.rm);
                    self.div8(bus, src);
                }
            }
            _ => {
                // IDIV (7)
                if word {
                    let src = self.read_rm16(bus, m.rm);
                    self.idiv16(bus, src);
                } else {
                    let src = self.read_rm8(bus, m.rm);
                    self.idiv8(bus, src);
                }
            }
        }
    }

    /// GRP4 (`0xFE`): INC/DEC r/m8.
    fn execute_grp4(&mut self, bus: &mut dyn CpuBus, seg: Option<u16>) -> bool {
        let m = self.decode_modrm(bus, seg);
        match m.reg {
            0 => {
                let a = self.read_rm8(bus, m.rm);
                let v = alu::inc8(&mut self.regs.flags, a);
                self.write_rm8(bus, m.rm, v);
            }
            1 => {
                let a = self.read_rm8(bus, m.rm);
                let v = alu::dec8(&mut self.regs.flags, a);
                self.write_rm8(bus, m.rm, v);
            }
            _ => return false,
        }
        true
    }

    /// GRP5 (`0xFF`): INC/DEC r/m16, indirect CALL/JMP (near/far), PUSH r/m16.
    fn execute_grp5(&mut self, bus: &mut dyn CpuBus, seg: Option<u16>) -> bool {
        let m = self.decode_modrm(bus, seg);
        match m.reg {
            0 => {
                let a = self.read_rm16(bus, m.rm);
                let v = alu::inc16(&mut self.regs.flags, a);
                self.write_rm16(bus, m.rm, v);
            }
            1 => {
                let a = self.read_rm16(bus, m.rm);
                let v = alu::dec16(&mut self.regs.flags, a);
                self.write_rm16(bus, m.rm, v);
            }
            2 => {
                // CALL near indirect
                let target = self.read_rm16(bus, m.rm);
                let ret = self.regs.ip;
                self.push16(bus, ret);
                self.regs.ip = target;
            }
            3 => {
                // CALL far indirect (memory operand only)
                let Rm::Memory {
                    segment, offset, ..
                } = m.rm
                else {
                    return false;
                };
                let new_ip = self.read_mem16(bus, segment, offset);
                let new_cs = self.read_mem16(bus, segment, offset.wrapping_add(2));
                let (cs, ip) = (self.regs.cs, self.regs.ip);
                self.push16(bus, cs);
                self.push16(bus, ip);
                self.regs.cs = new_cs;
                self.regs.ip = new_ip;
            }
            4 => {
                // JMP near indirect
                let target = self.read_rm16(bus, m.rm);
                self.regs.ip = target;
            }
            5 => {
                // JMP far indirect (memory operand only)
                let Rm::Memory {
                    segment, offset, ..
                } = m.rm
                else {
                    return false;
                };
                self.regs.ip = self.read_mem16(bus, segment, offset);
                self.regs.cs = self.read_mem16(bus, segment, offset.wrapping_add(2));
            }
            6 => {
                // PUSH Ev
                let v = self.read_rm16(bus, m.rm);
                self.push16(bus, v);
            }
            _ => return false,
        }
        true
    }

    /// Unsigned multiply: `AX = AL * src`.
    fn mul8(&mut self, src: u8) {
        let result = u16::from(self.regs.al()) * u16::from(src);
        self.regs.ax = result;
        let upper = (result >> 8) != 0;
        self.regs.flags.carry = upper;
        self.regs.flags.overflow = upper;
        // SF/ZF/PF/AF are officially undefined after MUL; left unchanged pending
        // WSCpuTest confirmation (see docs/hardware/01-cpu-v30mz.md appendix).
    }

    /// Unsigned multiply: `DX:AX = AX * src`.
    fn mul16(&mut self, src: u16) {
        let result = u32::from(self.regs.ax) * u32::from(src);
        self.regs.ax = result as u16;
        self.regs.dx = (result >> 16) as u16;
        let upper = self.regs.dx != 0;
        self.regs.flags.carry = upper;
        self.regs.flags.overflow = upper;
    }

    /// Signed multiply: `AX = AL * src`.
    fn imul8(&mut self, src: u8) {
        let result = i16::from(self.regs.al() as i8) * i16::from(src as i8);
        self.regs.ax = result as u16;
        let fits = (-128..=127).contains(&result);
        self.regs.flags.carry = !fits;
        self.regs.flags.overflow = !fits;
    }

    /// Signed multiply: `DX:AX = AX * src`.
    fn imul16(&mut self, src: u16) {
        let result = i32::from(self.regs.ax as i16) * i32::from(src as i16);
        self.regs.ax = result as u16;
        self.regs.dx = (result >> 16) as u16;
        let fits = (-32768..=32767).contains(&result);
        self.regs.flags.carry = !fits;
        self.regs.flags.overflow = !fits;
    }

    /// Deliver an interrupt/exception `vector`: push FLAGS, clear IF/TF, push
    /// CS:IP, then load the handler from the IVT at physical `vector * 4`
    /// (IP at `+0`, CS at `+2`). Wakes a halted CPU. Software `INT`, exceptions,
    /// and (via the machine) hardware IRQs all funnel through here.
    pub fn service_interrupt(&mut self, bus: &mut dyn CpuBus, vector: u8) {
        let flags = self.regs.flags.to_word();
        self.push16(bus, flags);
        self.regs.flags.interrupt = false;
        self.regs.flags.trap = false;
        let (cs, ip) = (self.regs.cs, self.regs.ip);
        self.push16(bus, cs);
        self.push16(bus, ip);
        let entry = u32::from(vector) * 4;
        self.regs.ip = bus.read_u16(entry);
        self.regs.cs = bus.read_u16(entry + 2);
        self.halted = false;
    }

    /// Unsigned divide: `AL = AX / src`, `AH = AX % src`. Raises `#DE` on
    /// divide-by-zero or quotient overflow (destination registers unchanged).
    fn div8(&mut self, bus: &mut dyn CpuBus, src: u8) {
        if src == 0 {
            self.service_interrupt(bus, 0);
            return;
        }
        let dividend = u32::from(self.regs.ax);
        let quotient = dividend / u32::from(src);
        if quotient > 0xFF {
            self.service_interrupt(bus, 0);
            return;
        }
        self.regs.set_al(quotient as u8);
        self.regs.set_ah((dividend % u32::from(src)) as u8);
    }

    /// Unsigned divide: `AX = DX:AX / src`, `DX = DX:AX % src`.
    fn div16(&mut self, bus: &mut dyn CpuBus, src: u16) {
        if src == 0 {
            self.service_interrupt(bus, 0);
            return;
        }
        let dividend = (u32::from(self.regs.dx) << 16) | u32::from(self.regs.ax);
        let quotient = dividend / u32::from(src);
        if quotient > 0xFFFF {
            self.service_interrupt(bus, 0);
            return;
        }
        self.regs.ax = quotient as u16;
        self.regs.dx = (dividend % u32::from(src)) as u16;
    }

    /// Signed divide (byte). Raises `#DE` on divide-by-zero or quotient overflow.
    fn idiv8(&mut self, bus: &mut dyn CpuBus, src: u8) {
        let divisor = i32::from(src as i8);
        if divisor == 0 {
            self.service_interrupt(bus, 0);
            return;
        }
        let dividend = i32::from(self.regs.ax as i16);
        let quotient = dividend / divisor;
        if !(-128..=127).contains(&quotient) {
            self.service_interrupt(bus, 0);
            return;
        }
        self.regs.set_al(quotient as u8);
        self.regs.set_ah((dividend % divisor) as u8);
    }

    /// Signed divide (word).
    fn idiv16(&mut self, bus: &mut dyn CpuBus, src: u16) {
        let divisor = i32::from(src as i16);
        if divisor == 0 {
            self.service_interrupt(bus, 0);
            return;
        }
        let dividend = ((u32::from(self.regs.dx) << 16) | u32::from(self.regs.ax)) as i32;
        // checked_div guards the INT_MIN / -1 overflow, which is also a #DE.
        let Some(quotient) = dividend.checked_div(divisor) else {
            self.service_interrupt(bus, 0);
            return;
        };
        if !(-32768..=32767).contains(&quotient) {
            self.service_interrupt(bus, 0);
            return;
        }
        self.regs.ax = quotient as u16;
        self.regs.dx = (dividend % divisor) as u16;
    }

    /// GRP2 (`C0/C1/D0/D1/D2/D3`): shifts and rotates.
    fn execute_grp2(&mut self, bus: &mut dyn CpuBus, opcode: u8, seg: Option<u16>) {
        let word = opcode & 1 == 1;
        let m = self.decode_modrm(bus, seg);
        let count: u8 = match opcode {
            0xD0 | 0xD1 => 1,
            0xD2 | 0xD3 => self.regs.cl(),
            _ => self.fetch_u8(bus), // C0 / C1 take an imm8 count
        };
        if word {
            let a = self.read_rm16(bus, m.rm);
            let r = alu::shift_rotate16(&mut self.regs.flags, m.reg, a, count);
            self.write_rm16(bus, m.rm, r);
        } else {
            let a = self.read_rm8(bus, m.rm);
            let r = alu::shift_rotate8(&mut self.regs.flags, m.reg, a, count);
            self.write_rm8(bus, m.rm, r);
        }
    }

    /// Execute a string instruction, honouring an optional REP/REPE/REPNE prefix.
    /// NOTE: a REP run completes in one call; real hardware allows interrupts
    /// mid-REP (revisit once interrupt delivery exists).
    fn execute_string(
        &mut self,
        bus: &mut dyn CpuBus,
        opcode: u8,
        seg: Option<u16>,
        rep: Option<u8>,
    ) {
        let Some(prefix) = rep else {
            self.string_iter(bus, opcode, seg);
            return;
        };
        let is_compare = matches!(opcode, 0xA6 | 0xA7 | 0xAE | 0xAF);
        loop {
            if self.regs.cx == 0 {
                break;
            }
            self.string_iter(bus, opcode, seg);
            self.regs.cx = self.regs.cx.wrapping_sub(1);
            if is_compare {
                let zero = self.regs.flags.zero;
                // REP/REPE (F3) continues while ZF=1; REPNE (F2) while ZF=0.
                if (prefix == 0xF3 && !zero) || (prefix == 0xF2 && zero) {
                    break;
                }
            }
        }
    }

    /// One iteration of a string instruction; advances SI/DI per the direction flag.
    fn string_iter(&mut self, bus: &mut dyn CpuBus, opcode: u8, seg: Option<u16>) {
        let word = opcode & 1 == 1;
        let step: i32 = if word { 2 } else { 1 };
        let delta = if self.regs.flags.direction {
            -step
        } else {
            step
        };
        let src_seg = seg.unwrap_or(self.regs.ds);
        match opcode {
            0xA4 | 0xA5 => {
                // MOVS: [ES:DI] <- [seg:SI]
                let (si, di, es) = (self.regs.si, self.regs.di, self.regs.es);
                if word {
                    let v = self.read_mem16(bus, src_seg, si);
                    self.write_mem16(bus, es, di, v);
                } else {
                    let v = bus.read_u8(physical_address(src_seg, si));
                    bus.write_u8(physical_address(es, di), v);
                }
                self.regs.si = Self::advanced(si, delta);
                self.regs.di = Self::advanced(di, delta);
            }
            0xAA | 0xAB => {
                // STOS: [ES:DI] <- AL/AX
                let (di, es) = (self.regs.di, self.regs.es);
                if word {
                    let ax = self.regs.ax;
                    self.write_mem16(bus, es, di, ax);
                } else {
                    let al = self.regs.al();
                    bus.write_u8(physical_address(es, di), al);
                }
                self.regs.di = Self::advanced(di, delta);
            }
            0xAC | 0xAD => {
                // LODS: AL/AX <- [seg:SI]
                let si = self.regs.si;
                if word {
                    let v = self.read_mem16(bus, src_seg, si);
                    self.regs.ax = v;
                } else {
                    let v = bus.read_u8(physical_address(src_seg, si));
                    self.regs.set_al(v);
                }
                self.regs.si = Self::advanced(si, delta);
            }
            0xA6 | 0xA7 => {
                // CMPS: CMP [seg:SI], [ES:DI]
                let (si, di, es) = (self.regs.si, self.regs.di, self.regs.es);
                if word {
                    let a = self.read_mem16(bus, src_seg, si);
                    let b = self.read_mem16(bus, es, di);
                    alu::cmp16(&mut self.regs.flags, a, b);
                } else {
                    let a = bus.read_u8(physical_address(src_seg, si));
                    let b = bus.read_u8(physical_address(es, di));
                    alu::cmp8(&mut self.regs.flags, a, b);
                }
                self.regs.si = Self::advanced(si, delta);
                self.regs.di = Self::advanced(di, delta);
            }
            _ => {
                // SCAS (0xAE/0xAF): CMP AL/AX, [ES:DI]
                let (di, es) = (self.regs.di, self.regs.es);
                if word {
                    let a = self.regs.ax;
                    let b = self.read_mem16(bus, es, di);
                    alu::cmp16(&mut self.regs.flags, a, b);
                } else {
                    let a = self.regs.al();
                    let b = bus.read_u8(physical_address(es, di));
                    alu::cmp8(&mut self.regs.flags, a, b);
                }
                self.regs.di = Self::advanced(di, delta);
            }
        }
    }

    /// Advance a string-op index register by `delta`, wrapping in 16 bits.
    fn advanced(reg: u16, delta: i32) -> u16 {
        (i32::from(reg) + delta) as u16
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
        io: Vec<u8>,
    }
    impl TestBus {
        fn new() -> Self {
            Self {
                mem: vec![0; (ADDR_MASK as usize) + 1],
                io: vec![0; 0x1_0000],
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
        fn io_read_u8(&mut self, p: u16) -> u8 {
            self.io[p as usize]
        }
        fn io_write_u8(&mut self, p: u16, v: u8) {
            self.io[p as usize] = v;
        }
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

    #[test]
    fn grp3_not_and_neg() {
        let mut bus = TestBus::new();
        // NOT AL (F6 /2, mod=11 rm=0) ; NEG AX (F7 /3, mod=11 rm=0)
        bus.load(0, &[0xF6, 0b11_010_000, 0xF7, 0b11_011_000]);
        let mut cpu = cpu();
        cpu.regs.set_al(0x0F);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0xF0);
        cpu.regs.ax = 0x0001;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0xFFFF);
        assert!(cpu.regs.flags.carry);
    }

    #[test]
    fn grp3_mul_unsigned_byte() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF6, 0b11_100_011]); // MUL BL (reg=4, rm=3=BL)
        let mut cpu = cpu();
        cpu.regs.set_al(0x10);
        cpu.regs.set_bl(0x10);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0x0100);
        assert!(cpu.regs.flags.carry && cpu.regs.flags.overflow);
    }

    #[test]
    fn grp3_mul_word_sets_dx() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF7, 0b11_100_001]); // MUL CX (reg=4, rm=1=CX)
        let mut cpu = cpu();
        cpu.regs.ax = 0x1000;
        cpu.regs.cx = 0x0010;
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.dx, cpu.regs.ax), (0x0001, 0x0000));
        assert!(cpu.regs.flags.carry);
    }

    #[test]
    fn grp3_imul_signed_byte() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF6, 0b11_101_011]); // IMUL BL (reg=5, rm=3=BL)
        let mut cpu = cpu();
        cpu.regs.set_al(0xFF); // -1
        cpu.regs.set_bl(0x02); // 2
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0xFFFE); // -2
        assert!(!cpu.regs.flags.carry, "-2 fits in a byte");
    }

    #[test]
    fn grp4_inc_memory_byte() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xFE, 0b00_000_111]); // INC byte [BX]
        let mut cpu = cpu();
        cpu.regs.ds = 0x2000;
        cpu.regs.bx = 0x0010;
        let addr = physical_address(0x2000, 0x0010);
        bus.write_u8(addr, 0x7F);
        cpu.step(&mut bus);
        assert_eq!(bus.read_u8(addr), 0x80);
        assert!(cpu.regs.flags.overflow, "0x7F+1 is a signed overflow");
    }

    #[test]
    fn grp5_indirect_near_call() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xFF, 0b00_010_111]); // CALL [BX] (FF /2, rm=7=BX)
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.regs.ds = 0x2000;
        cpu.regs.bx = 0x0040;
        bus.write_u8(physical_address(0x2000, 0x0040), 0x00);
        bus.write_u8(physical_address(0x2000, 0x0041), 0x12); // target 0x1200
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ip, 0x1200);
        assert_eq!(cpu.regs.sp, 0x00FE);
    }

    #[test]
    fn grp5_push_memory_word() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xFF, 0b00_110_111]); // PUSH [BX] (FF /6, rm=7=BX)
        let mut cpu = cpu();
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.regs.ds = 0x2000;
        cpu.regs.bx = 0x0040;
        bus.write_u8(physical_address(0x2000, 0x0040), 0xCD);
        bus.write_u8(physical_address(0x2000, 0x0041), 0xAB);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.sp, 0x00FE);
        assert_eq!(bus.read_u8(physical_address(0x3000, 0x00FE)), 0xCD);
        assert_eq!(bus.read_u8(physical_address(0x3000, 0x00FF)), 0xAB);
    }

    #[test]
    fn div8_quotient_and_remainder() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF6, 0b11_110_011]); // DIV BL (reg=6, rm=3=BL)
        let mut cpu = cpu();
        cpu.regs.ax = 0x0011; // 17
        cpu.regs.set_bl(0x05); // 5
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 3); // quotient
        assert_eq!(cpu.regs.ah(), 2); // remainder
    }

    #[test]
    fn idiv8_signed() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF6, 0b11_111_011]); // IDIV BL (reg=7, rm=3=BL)
        let mut cpu = cpu();
        cpu.regs.ax = 0xFFF7; // -9
        cpu.regs.set_bl(0x02); // 2
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al() as i8, -4); // -9 / 2 truncates toward zero
        assert_eq!(cpu.regs.ah() as i8, -1); // remainder -1
    }

    #[test]
    fn divide_by_zero_vectors_through_int0() {
        let mut bus = TestBus::new();
        bus.load(0x1000, &[0xF6, 0b11_110_011]); // DIV BL at 0x0100:0x0000
        // IVT entry 0 at physical 0: IP=0x1234, CS=0x8000
        bus.write_u8(0, 0x34);
        bus.write_u8(1, 0x12);
        bus.write_u8(2, 0x00);
        bus.write_u8(3, 0x80);
        let mut cpu = cpu();
        cpu.regs.cs = 0x0100;
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.regs.ax = 0x0011;
        cpu.regs.set_bl(0x00);
        cpu.step(&mut bus);
        assert_eq!((cpu.regs.cs, cpu.regs.ip), (0x8000, 0x1234));
        assert_eq!(cpu.regs.sp, 0x00FA, "pushed FLAGS + CS + IP");
    }

    #[test]
    fn int_and_iret_roundtrip() {
        let mut bus = TestBus::new();
        bus.load(0x1000, &[0xCD, 0x21]); // INT 0x21 at 0x0100:0x0000
        // IVT entry 0x21 at physical 0x84: IP=0x2000, CS=0x9000
        bus.write_u8(0x84, 0x00);
        bus.write_u8(0x85, 0x20);
        bus.write_u8(0x86, 0x00);
        bus.write_u8(0x87, 0x90);
        bus.write_u8(physical_address(0x9000, 0x2000), 0xCF); // IRET handler
        let mut cpu = cpu();
        cpu.regs.cs = 0x0100;
        cpu.regs.ss = 0x3000;
        cpu.regs.sp = 0x0100;
        cpu.regs.flags.interrupt = true;
        cpu.regs.flags.carry = true;
        cpu.step(&mut bus); // INT 0x21
        assert_eq!((cpu.regs.cs, cpu.regs.ip), (0x9000, 0x2000));
        assert!(!cpu.regs.flags.interrupt, "IF cleared on entry");
        cpu.step(&mut bus); // IRET
        assert_eq!((cpu.regs.cs, cpu.regs.ip), (0x0100, 0x0002));
        assert!(
            cpu.regs.flags.carry && cpu.regs.flags.interrupt,
            "flags restored"
        );
    }

    #[test]
    fn rep_stosb_fills_memory() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF3, 0xAA]); // REP STOSB
        let mut cpu = cpu();
        cpu.regs.es = 0x2000;
        cpu.regs.di = 0x0000;
        cpu.regs.set_al(0x5A);
        cpu.regs.cx = 3;
        cpu.step(&mut bus);
        for i in 0..3u16 {
            assert_eq!(bus.read_u8(physical_address(0x2000, i)), 0x5A);
        }
        assert_eq!(cpu.regs.di, 0x0003);
        assert_eq!(cpu.regs.cx, 0);
        assert_eq!(
            bus.read_u8(physical_address(0x2000, 3)),
            0x00,
            "stopped at CX=0"
        );
    }

    #[test]
    fn movsb_copies_and_advances() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xA4]); // MOVSB
        let mut cpu = cpu();
        cpu.regs.ds = 0x1000;
        cpu.regs.si = 0x0010;
        cpu.regs.es = 0x2000;
        cpu.regs.di = 0x0020;
        bus.write_u8(physical_address(0x1000, 0x0010), 0x99);
        cpu.step(&mut bus);
        assert_eq!(bus.read_u8(physical_address(0x2000, 0x0020)), 0x99);
        assert_eq!((cpu.regs.si, cpu.regs.di), (0x0011, 0x0021));
    }

    #[test]
    fn lodsw_loads_ax_and_advances_by_two() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xAD]); // LODSW
        let mut cpu = cpu();
        cpu.regs.ds = 0x1000;
        cpu.regs.si = 0x0000;
        bus.write_u8(physical_address(0x1000, 0), 0xCD);
        bus.write_u8(physical_address(0x1000, 1), 0xAB);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.ax, 0xABCD);
        assert_eq!(cpu.regs.si, 0x0002);
    }

    #[test]
    fn direction_flag_decrements_index() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xAA]); // STOSB
        let mut cpu = cpu();
        cpu.regs.es = 0x2000;
        cpu.regs.di = 0x0010;
        cpu.regs.set_al(0x11);
        cpu.regs.flags.direction = true;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.di, 0x000F, "DF=1 decrements DI");
    }

    #[test]
    fn out_then_in_immediate_port() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xE6, 0x40, 0xE4, 0x40]); // OUT 0x40,AL ; IN AL,0x40
        let mut cpu = cpu();
        cpu.regs.set_al(0xAB);
        cpu.step(&mut bus);
        assert_eq!(bus.io[0x40], 0xAB);
        cpu.regs.set_al(0x00);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0xAB);
    }

    #[test]
    fn in_out_via_dx_port() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xEE, 0xEC]); // OUT DX,AL ; IN AL,DX
        let mut cpu = cpu();
        cpu.regs.dx = 0x00B5;
        cpu.regs.set_al(0x5A);
        cpu.step(&mut bus);
        assert_eq!(bus.io[0xB5], 0x5A);
        cpu.regs.set_al(0x00);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x5A);
    }

    #[test]
    fn repne_scasb_stops_on_match() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xF2, 0xAE]); // REPNE SCASB
        let mut cpu = cpu();
        cpu.regs.es = 0x2000;
        cpu.regs.di = 0x0000;
        cpu.regs.cx = 8;
        cpu.regs.set_al(0x42);
        bus.write_u8(physical_address(0x2000, 2), 0x42); // match at offset 2
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.di, 0x0003, "advanced past the match");
        assert_eq!(cpu.regs.cx, 5);
        assert!(cpu.regs.flags.zero);
    }

    #[test]
    fn shl_by_one() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xD0, 0b11_100_000]); // SHL AL,1 (D0 /4, rm=0=AL)
        let mut cpu = cpu();
        cpu.regs.set_al(0x40);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x80);
        assert!(!cpu.regs.flags.carry && cpu.regs.flags.overflow && cpu.regs.flags.sign);
    }

    #[test]
    fn shr_by_cl() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xD2, 0b11_101_000]); // SHR AL,CL (D2 /5, rm=0=AL)
        let mut cpu = cpu();
        cpu.regs.set_al(0x80);
        cpu.regs.set_cl(4);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x08);
    }

    #[test]
    fn sar_preserves_sign() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xD0, 0b11_111_000]); // SAR AL,1 (D0 /7)
        let mut cpu = cpu();
        cpu.regs.set_al(0x80);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0xC0);
    }

    #[test]
    fn rol_then_ror() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xD0, 0b11_000_000, 0xD0, 0b11_001_000]); // ROL AL,1 ; ROR AL,1
        let mut cpu = cpu();
        cpu.regs.set_al(0x80);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x01);
        assert!(cpu.regs.flags.carry);
        cpu.regs.set_al(0x01);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x80);
        assert!(cpu.regs.flags.carry);
    }

    #[test]
    fn shl_word_by_immediate_count() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xC1, 0b11_100_011, 0x04]); // SHL BX,4 (C1 /4, rm=3=BX)
        let mut cpu = cpu();
        cpu.regs.bx = 0x0001;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.bx, 0x0010);
    }

    #[test]
    fn shift_count_zero_leaves_flags() {
        let mut bus = TestBus::new();
        bus.load(0, &[0xD2, 0b11_100_000]); // SHL AL,CL with CL=0
        let mut cpu = cpu();
        cpu.regs.set_al(0x40);
        cpu.regs.set_cl(0);
        cpu.regs.flags.carry = true;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.al(), 0x40, "no change");
        assert!(cpu.regs.flags.carry, "flags untouched by a count of 0");
    }
}
