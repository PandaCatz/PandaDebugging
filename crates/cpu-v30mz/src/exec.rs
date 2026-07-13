//! Instruction execution: `step()` fetches, decodes, and executes one
//! instruction. This is the growing opcode table; each increment adds a
//! coherent block with tests. **No cycle counting yet** — the master-vs-CPU
//! cycle-unit question (see `docs/hardware/01-cpu-v30mz.md`) is unresolved.
//!
//! Implemented so far: segment-override / LOCK prefixes, the ALU opcode block
//! (`ADD/OR/ADC/SBB/AND/SUB/XOR/CMP`, opcodes `0x00–0x3D`, all six operand
//! forms), and the flag / `NOP` / `HLT` opcodes.

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
}
