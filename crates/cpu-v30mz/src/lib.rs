#![forbid(unsafe_code)]

//! NEC V30MZ CPU core — work in progress.
//!
//! Implemented (spec-grounded, timing-independent): the register file, flags,
//! 20-bit segmented addressing, the confirmed `CS:IP = FFFF:0000` reset state,
//! the trace-first [`CpuBus`] interface, and instruction-stream fetch.
//!
//! Not implemented yet (next increment): opcode decode, execution, and the
//! interrupt-delivery sequence. **Timing is deliberately absent** until the
//! master-clock-vs-CPU-clock cycle-unit question is resolved on hardware — see
//! the preamble of `docs/hardware/01-cpu-v30mz.md`. No cycle counts are baked in.

pub mod bus;
pub mod decode;
pub mod registers;

pub use bus::CpuBus;
pub use decode::{ModRm, Rm};
pub use registers::{Flags, Registers, physical_address};

/// The processor: architectural state only. It reaches the outside world
/// exclusively through a [`CpuBus`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cpu {
    pub regs: Registers,
    /// Set by `HLT`; the machine resumes the CPU on the next interrupt.
    pub halted: bool,
}

impl Cpu {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            regs: Registers::reset(),
            halted: false,
        }
    }

    /// Return to the hard-reset state.
    pub const fn reset(&mut self) {
        self.regs = Registers::reset();
        self.halted = false;
    }

    /// Fetch the next instruction byte at `CS:IP` and advance `IP`, wrapping the
    /// offset within the code segment (the V30MZ prefetch does not cross into
    /// the next segment).
    pub fn fetch_u8(&mut self, bus: &mut dyn CpuBus) -> u8 {
        let address = self.regs.code_address();
        self.regs.ip = self.regs.ip.wrapping_add(1);
        bus.read_u8(address)
    }

    /// Fetch the next little-endian instruction word and advance `IP` by two.
    pub fn fetch_u16(&mut self, bus: &mut dyn CpuBus) -> u16 {
        let lo = self.fetch_u8(bus);
        let hi = self.fetch_u8(bus);
        u16::from_le_bytes([lo, hi])
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat 1 MiB memory with inert I/O, enough to exercise fetch and access.
    struct TestBus {
        mem: Vec<u8>,
    }

    impl TestBus {
        fn new() -> Self {
            Self {
                mem: vec![0; (bus::ADDR_MASK as usize) + 1],
            }
        }
    }

    impl CpuBus for TestBus {
        fn read_u8(&mut self, address: u32) -> u8 {
            self.mem[(address & bus::ADDR_MASK) as usize]
        }
        fn write_u8(&mut self, address: u32, value: u8) {
            self.mem[(address & bus::ADDR_MASK) as usize] = value;
        }
        fn io_read_u8(&mut self, _port: u16) -> u8 {
            0
        }
        fn io_write_u8(&mut self, _port: u16, _value: u8) {}
    }

    #[test]
    fn fetch_reads_from_reset_vector_and_advances_ip() {
        let mut bus = TestBus::new();
        // Bytes at the reset vector 0xFFFF0.
        bus.write_u8(0xFFFF0, 0xEA);
        bus.write_u8(0xFFFF1, 0x34);
        bus.write_u8(0xFFFF2, 0x12);
        let mut cpu = Cpu::new();
        assert_eq!(cpu.fetch_u8(&mut bus), 0xEA);
        assert_eq!(cpu.regs.ip, 1);
        assert_eq!(cpu.fetch_u16(&mut bus), 0x1234);
        assert_eq!(cpu.regs.ip, 3);
    }

    #[test]
    fn default_word_access_is_little_endian() {
        let mut bus = TestBus::new();
        bus.write_u8(0x100, 0xCD);
        bus.write_u8(0x101, 0xAB);
        assert_eq!(bus.read_u16(0x100), 0xABCD);
        bus.write_u16(0x200, 0xBEEF);
        assert_eq!(bus.read_u16(0x200), 0xBEEF);
    }

    #[test]
    fn reset_clears_halt_and_restores_vector() {
        let mut cpu = Cpu::new();
        cpu.halted = true;
        cpu.regs.ip = 0x1234;
        cpu.reset();
        assert!(!cpu.halted);
        assert_eq!(cpu.regs.code_address(), 0xFFFF0);
    }
}
