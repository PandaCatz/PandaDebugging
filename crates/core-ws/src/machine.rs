//! A minimal WonderSwan machine: it wires the V30MZ to memory, the I/O ports,
//! and the interrupt controller, and delivers hardware IRQs before each step.
//!
//! The memory map here is a **placeholder flat 1 MiB** — enough to run and test
//! CPU + interrupt integration end-to-end. The real WonderSwan map (internal RAM
//! sizing per model, ROM/SRAM banking, the full I/O register file, the PPU, and
//! timers) is future work and must be built from verified WSMan details, not
//! guessed. Only the doc-cited interrupt registers are wired to I/O so far.

use crate::interrupt::InterruptController;
use crate::io;
use cpu_v30mz::{Cpu, CpuBus, Step};

/// Flat placeholder memory size (power of two so masking wraps cleanly).
const MEMORY_SIZE: usize = 0x10_0000;

/// Memory + I/O + the interrupt controller, reached by the CPU via [`CpuBus`].
pub struct Bus {
    memory: Vec<u8>,
    interrupts: InterruptController,
}

impl Bus {
    fn new() -> Self {
        Self {
            memory: vec![0; MEMORY_SIZE],
            interrupts: InterruptController::new(),
        }
    }
}

impl CpuBus for Bus {
    fn read_u8(&mut self, address: u32) -> u8 {
        self.memory[(address as usize) & (MEMORY_SIZE - 1)]
    }

    fn write_u8(&mut self, address: u32, value: u8) {
        self.memory[(address as usize) & (MEMORY_SIZE - 1)] = value;
    }

    fn io_read_u8(&mut self, port: u16) -> u8 {
        match port {
            io::REG_INT_BASE => self.interrupts.base(),
            // Placeholder: real WS returns $90 (WS) / $00 (WSC) for unmapped
            // ports, and other registers are not modelled yet.
            _ => 0x90,
        }
    }

    fn io_write_u8(&mut self, port: u16, value: u8) {
        match port {
            io::REG_INT_BASE => self.interrupts.set_base(value),
            io::REG_INT_ACK => self.interrupts.acknowledge(value),
            _ => {}
        }
    }
}

/// The machine: a CPU plus its bus. Owns all mutable running state.
pub struct Machine {
    cpu: Cpu,
    bus: Bus,
}

impl Machine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(),
        }
    }

    /// Copy `bytes` into memory at physical `address` (loader/test helper).
    pub fn load(&mut self, address: u32, bytes: &[u8]) {
        let base = address as usize;
        self.bus.memory[base..base + bytes.len()].copy_from_slice(bytes);
    }

    #[must_use]
    pub fn read(&self, address: u32) -> u8 {
        self.bus.memory[(address as usize) & (MEMORY_SIZE - 1)]
    }

    #[must_use]
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    pub fn interrupts_mut(&mut self) -> &mut InterruptController {
        &mut self.bus.interrupts
    }

    /// Deliver the highest-priority pending, enabled hardware IRQ (when the CPU's
    /// `IF` is set), then execute one instruction. An interrupt also wakes a
    /// halted CPU (via [`Cpu::service_interrupt`]).
    pub fn step(&mut self) -> Step {
        if self.cpu.regs.flags.interrupt
            && let Some(irq) = self.bus.interrupts.pending_enabled()
        {
            let vector = self.bus.interrupts.vector(irq);
            self.cpu.service_interrupt(&mut self.bus, vector);
            // No auto-ack: hardware requires the ISR to write REG_INT_ACK ($B6)
            // to clear edge lines. service_interrupt clears IF, so the line will
            // not re-fire until the ISR re-enables IF (by which point a
            // well-behaved handler has acked).
        }
        self.cpu.step(&mut self.bus)
    }
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interrupt::Irq;

    fn seed_cpu(m: &mut Machine, ip: u16) {
        let cpu = m.cpu_mut();
        cpu.regs.cs = 0;
        cpu.regs.ip = ip;
        cpu.regs.ss = 0;
        cpu.regs.sp = 0x0400;
    }

    #[test]
    fn hardware_irq_delivered_acked_and_returns() {
        let mut m = Machine::new();
        m.load(0x0100, &[0x90, 0x90, 0xF4]); // main: NOP; NOP; HLT
        // handler: MOV AL,0x40 ; OUT 0xB6,AL (ack Vblank) ; IRET
        m.load(0x0200, &[0xB0, 0x40, 0xE6, 0xB6, 0xCF]);
        m.interrupts_mut().set_base(0x20); // Vblank (bit 6) -> vector 0x26
        m.load(0x26 * 4, &[0x00, 0x02, 0x00, 0x00]); // IVT[0x26] = 0000:0200
        seed_cpu(&mut m, 0x0100);
        m.cpu_mut().regs.flags.interrupt = true;
        m.interrupts_mut().set_enable(Irq::Vblank.bit());
        m.interrupts_mut().raise(Irq::Vblank);

        m.step(); // deliver IRQ, then run handler's MOV AL,0x40
        assert_eq!(m.cpu().regs.al(), 0x40, "handler ran");
        assert!(
            !m.cpu().regs.flags.interrupt,
            "IF cleared inside the handler"
        );

        m.step(); // OUT 0xB6,AL -> ISR acknowledges the line
        assert_eq!(
            m.interrupts_mut().status() & Irq::Vblank.bit(),
            0,
            "the ISR acked the edge line via REG_INT_ACK"
        );

        m.step(); // IRET back to main
        assert_eq!(
            m.cpu().regs.ip,
            0x0100,
            "returned to the interrupted program"
        );
        assert!(m.cpu().regs.flags.interrupt, "IF restored by IRET");

        m.step(); // no re-delivery: main NOP runs
        assert_eq!(m.cpu().regs.ip, 0x0101, "no spurious re-delivery after ack");
    }

    #[test]
    fn irq_withheld_when_interrupts_disabled() {
        let mut m = Machine::new();
        m.load(0x0100, &[0x90]); // NOP
        seed_cpu(&mut m, 0x0100);
        m.cpu_mut().regs.flags.interrupt = false; // IF clear
        m.interrupts_mut().set_enable(Irq::Vblank.bit());
        m.interrupts_mut().raise(Irq::Vblank);
        m.step();
        assert_eq!(m.cpu().regs.ip, 0x0101, "no vectoring; only the NOP ran");
    }

    #[test]
    fn int_base_register_is_writable_via_io() {
        let mut m = Machine::new();
        m.load(0x0100, &[0xB0, 0x40, 0xE6, 0xB0]); // MOV AL,0x40 ; OUT 0xB0,AL
        seed_cpu(&mut m, 0x0100);
        m.step(); // MOV
        m.step(); // OUT
        assert_eq!(m.interrupts_mut().base(), 0x40);
    }
}
