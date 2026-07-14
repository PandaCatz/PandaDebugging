// SPDX-License-Identifier: GPL-3.0-or-later
//! The trace-first interface the CPU uses to reach memory and I/O.
//!
//! The CPU never owns RAM, ROM, or devices; it calls out through `CpuBus` so the
//! same core can be driven by the real machine, by a test harness, or by a trace
//! recorder. The CPU passes **physical 20-bit addresses** (it has already done
//! segment translation); the bus decodes them.
//!
//! Word helpers default to little-endian access of two consecutive physical
//! bytes with a 1 MiB wrap. A bus that models the 4-slot memory arbitration (see
//! `docs/hardware/01-cpu-v30mz.md` §memory) may override them. Segment-*offset*
//! wrap (e.g. a word operand at offset `0xFFFF`) is the CPU's responsibility and
//! is handled where operands are decoded, not here.

/// Physical address space mask (20-bit, 1 MiB).
pub const ADDR_MASK: u32 = 0x000F_FFFF;

pub trait CpuBus {
    /// Read one byte at a physical address.
    fn read_u8(&mut self, address: u32) -> u8;
    /// Write one byte at a physical address.
    fn write_u8(&mut self, address: u32, value: u8);
    /// Read one byte from an I/O port (`$000–$0FF` decoded by the bus).
    fn io_read_u8(&mut self, port: u16) -> u8;
    /// Write one byte to an I/O port.
    fn io_write_u8(&mut self, port: u16, value: u8);

    /// Little-endian word read of two consecutive physical bytes (1 MiB wrap).
    fn read_u16(&mut self, address: u32) -> u16 {
        let lo = self.read_u8(address);
        let hi = self.read_u8(address.wrapping_add(1) & ADDR_MASK);
        u16::from_le_bytes([lo, hi])
    }

    /// Little-endian word write of two consecutive physical bytes (1 MiB wrap).
    fn write_u16(&mut self, address: u32, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.write_u8(address, lo);
        self.write_u8(address.wrapping_add(1) & ADDR_MASK, hi);
    }

    /// Little-endian word read from consecutive I/O ports.
    fn io_read_u16(&mut self, port: u16) -> u16 {
        let lo = self.io_read_u8(port);
        let hi = self.io_read_u8(port.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    /// Little-endian word write to consecutive I/O ports.
    fn io_write_u16(&mut self, port: u16, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.io_write_u8(port, lo);
        self.io_write_u8(port.wrapping_add(1), hi);
    }
}
