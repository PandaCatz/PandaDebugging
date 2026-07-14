//! A minimal WonderSwan machine: it wires the V30MZ to the real address-routing
//! memory map ([`MemoryMap`]), the I/O ports, and the interrupt controller, and
//! delivers hardware IRQs before each step.
//!
//! The interrupt ports (`$B0`/`$B6`) are serviced here because the machine owns
//! the [`InterruptController`]; every other memory and I/O access is routed by
//! the [`MemoryMap`]. The PPU, APU, DMA engines, and cycle timing are not yet
//! wired.

use crate::cartridge::WsCartridge;
use crate::interrupt::InterruptController;
use crate::io;
use crate::memory::MemoryMap;
use cpu_v30mz::{Cpu, CpuBus, Step};
use ws_contracts::Model;

/// The routed address space plus the interrupt controller, reached by the CPU
/// via [`CpuBus`].
pub struct Bus {
    map: MemoryMap,
    interrupts: InterruptController,
}

impl Bus {
    fn new(model: Model, cart: Option<WsCartridge>) -> Self {
        Self {
            map: MemoryMap::new(model, cart),
            interrupts: InterruptController::new(),
        }
    }
}

impl CpuBus for Bus {
    fn read_u8(&mut self, address: u32) -> u8 {
        self.map.read_u8(address)
    }

    fn write_u8(&mut self, address: u32, value: u8) {
        self.map.write_u8(address, value);
    }

    fn io_read_u8(&mut self, port: u16) -> u8 {
        // The machine owns the interrupt controller, so it services $B0; the
        // memory map routes everything else.
        match port {
            io::REG_INT_BASE => self.interrupts.base(),
            _ => self.map.io_read(port),
        }
    }

    fn io_write_u8(&mut self, port: u16, value: u8) {
        match port {
            io::REG_INT_BASE => self.interrupts.set_base(value),
            io::REG_INT_ACK => self.interrupts.acknowledge(value),
            _ => self.map.io_write(port, value),
        }
    }
}

/// The machine: a CPU plus its bus. Owns all mutable running state.
pub struct Machine {
    cpu: Cpu,
    bus: Bus,
}

impl Machine {
    /// A machine with no cartridge, defaulting to the colour model (64 KiB RAM
    /// covering the whole internal region). Convenient for CPU/interrupt
    /// integration tests that load code straight into RAM.
    #[must_use]
    pub fn new() -> Self {
        Self::with(Model::Color, None)
    }

    /// A machine for `model` running `cart`.
    #[must_use]
    pub fn with_cartridge(model: Model, cart: WsCartridge) -> Self {
        Self::with(model, Some(cart))
    }

    fn with(model: Model, cart: Option<WsCartridge>) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(model, cart),
        }
    }

    /// Copy `bytes` into memory starting at physical `address` (loader/test
    /// helper). Routed through the memory map, so only writable regions take.
    pub fn load(&mut self, address: u32, bytes: &[u8]) {
        for (offset, &byte) in bytes.iter().enumerate() {
            self.bus.map.write_u8(address + offset as u32, byte);
        }
    }

    #[must_use]
    pub fn read(&self, address: u32) -> u8 {
        self.bus.map.read_u8(address)
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

    #[test]
    fn boots_from_cartridge_rom_via_the_reset_vector() {
        // Footer boot far-jump `JMP FAR 2000:0000` → physical 0x20000, the ROM0
        // window (bank 0 → ROM offset 0), where we place a NOP.
        let mut rom = vec![0u8; 64 * 1024];
        rom[0x0000] = 0x90; // NOP, reached after the jump
        let base = rom.len() - format_ws::HEADER_LEN;
        rom[base..base + 5].copy_from_slice(&[0xEA, 0x00, 0x00, 0x00, 0x20]);
        let cart = WsCartridge::from_bytes(&rom).expect("valid cart");
        let mut m = Machine::with_cartridge(Model::Color, cart);

        // Reset state CS:IP = FFFF:0000 fetches the footer far-jump from ROM.
        m.step();
        assert_eq!(m.cpu().regs.cs, 0x2000, "far-jumped into cartridge ROM");
        assert_eq!(m.cpu().regs.ip, 0x0000);

        m.step(); // the NOP at ROM offset 0
        assert_eq!(m.cpu().regs.ip, 0x0001, "executed an instruction from ROM");
    }
}
