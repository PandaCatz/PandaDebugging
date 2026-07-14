// SPDX-License-Identifier: GPL-3.0-or-later
//! The WonderSwan physical address space — the real memory map.
//!
//! Replaces the earlier placeholder flat 1 MiB bus. Routing follows the verified
//! spec in `docs/hardware/01-cpu-v30mz.md` (§"CPU memory map, I/O port
//! mechanics"):
//!
//! - **`0x00000–0x0FFFF` internal RAM**, sized by model (16 KiB mono / 64 KiB
//!   colour). Above the physical RAM on mono (`0x04000–0x0FFFF`) is an explicit
//!   *open question* in the sources — not fabricated here.
//! - **`0x10000–0x1FFFF` cartridge SRAM window** (banked by `$C1`), backed by a
//!   store sized from the cartridge footer's save-type code.
//! - **`0x20000–0xFFFFF` cartridge ROM** through the three bank windows
//!   (`$C2` ROM0, `$C3` ROM1, `$C0` linear), read-only.
//! - **I/O three-way decode**: `$B8–$BF` internal EEPROM, `$C0–$FF` cartridge
//!   bus, else the SoC block (low-9-bit decode `≤ $B7`), else open bus.
//! - **`$A0` system control** (boot-ROM lockout latch, bus width/speed, colour
//!   status), and model-dependent **open-bus** reads (`$90` mono / `$00` colour).
//!
//! Deferred as explicit gaps (not guessed): the boot-ROM overlay (HLE — cartridge
//! ROM shows through the top region), the internal-EEPROM register protocol, the
//! cartridge RTC / external-EEPROM ports, most SoC registers, and all cycle
//! timing. Interrupt ports (`$B0`/`$B6`) are handled by the owning machine.

use crate::cartridge::WsCartridge;
use crate::io;
use format_ws::{BusWidth, MapperKind, SaveKind};
use ws_contracts::Model;

const RAM_TOP: u32 = 0x0_FFFF;
const SRAM_BASE: u32 = 0x1_0000;
const SRAM_TOP: u32 = 0x1_FFFF;
const ROM0_BASE: u32 = 0x2_0000;
const ROM1_BASE: u32 = 0x3_0000;

/// Which I/O block a 16-bit port decodes to (§4 routing).
enum IoBlock {
    Eeprom,
    Cartridge,
    Soc,
    OpenBus,
}

/// The four standard mapper bank-select registers (`$C0–$C3`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Banks {
    linear: u8, // $C0 (EX / linear)
    sram: u8,   // $C1
    rom0: u8,   // $C2
    rom1: u8,   // $C3
    is_2003: bool,
}

impl Banks {
    fn power_up(is_2003: bool) -> Self {
        // HLE defaults (no boot ROM). $C3 (ROM1) powers up $FF on hardware (WSdev
        // Mapper). $C0 (linear) is also set to $FF so the top of the cartridge
        // ROM maps to the reset vector (0xFFFF0) for *every* ROM size — the state
        // the boot ROM would leave. With $C0 = 0 the vector only reaches the
        // footer for ROMs ≤ 1 MiB; larger carts would fetch a garbage first
        // instruction. Games reprogram the banks before use.
        Self {
            linear: 0xFF,
            sram: 0,
            rom0: 0,
            rom1: 0xFF,
            is_2003,
        }
    }

    /// `$C0` readback mask: 6 bits on the Bandai 2003 mapper, 4 on the 2001;
    /// high bits read back as 0.
    const fn linear_mask(&self) -> u8 {
        if self.is_2003 { 0x3F } else { 0x0F }
    }
}

/// The routed WonderSwan address space for one session.
pub struct MemoryMap {
    model: Model,
    ram: Vec<u8>,
    sram: Vec<u8>,
    cart: Option<WsCartridge>,
    banks: Banks,
    /// `$A0` writable bits only (0 = lockout latch, 2 = bus width, 3 = speed).
    /// Status bits (1 = colour, 7 = self-test) are merged in on read.
    system_ctrl: u8,
}

impl MemoryMap {
    #[must_use]
    pub fn new(model: Model, cart: Option<WsCartridge>) -> Self {
        let ram = vec![0u8; ram_size(model)];

        let (sram, is_2003, bus16) = match &cart {
            Some(c) => {
                let header = c.header();
                let sram = match header.save_type().kind() {
                    SaveKind::Sram => vec![0u8; header.save_type().bytes().unwrap_or(0) as usize],
                    _ => Vec::new(),
                };
                let is_2003 = header.mapper().kind() == MapperKind::Bandai2003;
                let bus16 = header.bus_width() == BusWidth::Sixteen;
                (sram, is_2003, bus16)
            }
            None => (Vec::new(), false, false),
        };

        // HLE default (no boot ROM): reflect the cartridge's declared ROM bus
        // width into `$A0` bit 2, the configuration the BIOS would apply.
        let system_ctrl = if bus16 { 0x04 } else { 0x00 };

        Self {
            model,
            ram,
            sram,
            cart,
            banks: Banks::power_up(is_2003),
            system_ctrl,
        }
    }

    /// The model's open-bus byte for unmapped reads (`$90` mono / `$00` colour,
    /// native colour mode).
    const fn open_bus(&self) -> u8 {
        match self.model {
            Model::Mono => 0x90,
            Model::Color | Model::Crystal => 0x00,
        }
    }

    // --- memory bus ---------------------------------------------------------

    #[must_use]
    pub fn read_u8(&self, address: u32) -> u8 {
        match address & 0xF_FFFF {
            0x0_0000..=RAM_TOP => self.read_ram(address & 0xF_FFFF),
            SRAM_BASE..=SRAM_TOP => self.read_sram(address & 0xF_FFFF),
            rom => self.read_rom(rom),
        }
    }

    pub fn write_u8(&mut self, address: u32, value: u8) {
        match address & 0xF_FFFF {
            0x0_0000..=RAM_TOP => {
                let idx = (address & 0xF_FFFF) as usize;
                if idx < self.ram.len() {
                    self.ram[idx] = value;
                }
                // Above physical RAM (mono `0x04000+`) is unverified — drop.
            }
            SRAM_BASE..=SRAM_TOP if !self.sram.is_empty() => {
                let off = self.sram_offset(address & 0xF_FFFF);
                self.sram[off] = value;
            }
            // Cartridge ROM (`0x20000–0xFFFFF`) is read-only; SRAM window with no
            // backing store also lands here and is ignored.
            _ => {}
        }
    }

    fn read_ram(&self, address: u32) -> u8 {
        let idx = address as usize;
        if idx < self.ram.len() {
            self.ram[idx]
        } else {
            // Mono `0x04000–0x0FFFF`: no physical RAM; behaviour (mirror vs
            // undefined) is an open question in the sources. Return open bus
            // rather than fabricate a mirror.
            self.open_bus()
        }
    }

    fn sram_offset(&self, address: u32) -> usize {
        let flat = (u32::from(self.banks.sram) << 16) | (address & 0xFFFF);
        (flat as usize) % self.sram.len()
    }

    fn read_sram(&self, address: u32) -> u8 {
        if self.sram.is_empty() {
            return self.open_bus();
        }
        self.sram[self.sram_offset(address)]
    }

    fn read_rom(&self, address: u32) -> u8 {
        let Some(cart) = &self.cart else {
            return self.open_bus();
        };
        let rom = cart.rom();
        if rom.is_empty() {
            return self.open_bus();
        }
        // Each window selects a bank; the ROM mirrors when smaller than the
        // window (address masked modulo the ROM length).
        let flat = match address {
            ROM0_BASE..=0x2_FFFF => (u32::from(self.banks.rom0) << 16) | (address & 0xFFFF),
            ROM1_BASE..=0x3_FFFF => (u32::from(self.banks.rom1) << 16) | (address & 0xFFFF),
            // Linear window `0x40000–0xFFFFF`: bank << 20 | offset-within-1MiB.
            // The bank is masked to the register's implemented width (4 bits on
            // the 2001 mapper, 6 on 2003), matching each mapper's ROM ceiling.
            _ => {
                let bank = u32::from(self.banks.linear & self.banks.linear_mask());
                (bank << 20) | (address & 0xF_FFFF)
            }
        };
        rom[(flat as usize) % rom.len()]
    }

    // --- I/O bus (interrupt ports $B0/$B6 handled by the machine) -----------

    fn route(port: u16) -> IoBlock {
        if (0x00B8..=0x00BF).contains(&port) {
            IoBlock::Eeprom
        } else if (0x00C0..=0x00FF).contains(&port) {
            IoBlock::Cartridge
        } else if (port & 0x01FF) <= 0x00B7 {
            // SoC decode keys off only the low 9 bits, so SoC ports alias every
            // 512 entries across the 16-bit port space.
            IoBlock::Soc
        } else {
            IoBlock::OpenBus
        }
    }

    #[must_use]
    pub fn io_read(&self, port: u16) -> u8 {
        match Self::route(port) {
            IoBlock::Cartridge => match port {
                io::REG_BANK_LINEAR => self.banks.linear & self.banks.linear_mask(),
                io::REG_BANK_SRAM => self.banks.sram,
                io::REG_BANK_ROM0 => self.banks.rom0,
                io::REG_BANK_ROM1 => self.banks.rom1,
                // Other cartridge ports (RTC, external EEPROM, $CE) not modelled.
                _ => self.open_bus(),
            },
            IoBlock::Soc if port & 0x00FF == io::REG_SYSTEM_CTRL => self.read_system_ctrl(),
            // SoC register not modelled yet, internal EEPROM not wired — explicit
            // gaps, returned as open bus rather than a guessed value.
            IoBlock::Soc | IoBlock::Eeprom | IoBlock::OpenBus => self.open_bus(),
        }
    }

    pub fn io_write(&mut self, port: u16, value: u8) {
        match Self::route(port) {
            IoBlock::Cartridge => match port {
                io::REG_BANK_LINEAR => self.banks.linear = value,
                io::REG_BANK_SRAM => self.banks.sram = value,
                io::REG_BANK_ROM0 => self.banks.rom0 = value,
                io::REG_BANK_ROM1 => self.banks.rom1 = value,
                _ => {}
            },
            IoBlock::Soc if port & 0x00FF == io::REG_SYSTEM_CTRL => self.write_system_ctrl(value),
            IoBlock::Soc | IoBlock::Eeprom | IoBlock::OpenBus => {}
        }
    }

    fn read_system_ctrl(&self) -> u8 {
        let mut value = self.system_ctrl & 0x0D; // bits 0, 2, 3
        if matches!(self.model, Model::Color | Model::Crystal) {
            value |= 0x02; // bit 1: colour system
        }
        if self.cart.is_some() {
            value |= 0x80; // bit 7: cartridge / self-test OK
        }
        value
    }

    fn write_system_ctrl(&mut self, value: u8) {
        // Bit 0 is a one-way boot-ROM lockout latch (0→1, never back to 0);
        // bits 2, 3 are freely writable; bits 1, 4–7 are not writable here.
        let locked = (self.system_ctrl | value) & 0x01;
        self.system_ctrl = locked | (value & 0x0C);
    }
}

fn ram_size(model: Model) -> usize {
    match model {
        Model::Mono => 16 * 1024,
        Model::Color | Model::Crystal => 64 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn footer_rom(len: usize, boot: [u8; 5]) -> Vec<u8> {
        let mut rom = vec![0u8; len];
        let base = len - format_ws::HEADER_LEN;
        rom[base..base + 5].copy_from_slice(&boot);
        rom
    }

    #[test]
    fn ram_round_trips_and_sizes_by_model() {
        let mut mono = MemoryMap::new(Model::Mono, None);
        let mut color = MemoryMap::new(Model::Color, None);
        mono.write_u8(0x0100, 0xAB);
        color.write_u8(0x0100, 0xCD);
        assert_eq!(mono.read_u8(0x0100), 0xAB);
        assert_eq!(color.read_u8(0x0100), 0xCD);
        // Colour RAM fills the whole region; mono only the low 16 KiB.
        color.write_u8(0x8000, 0x11);
        assert_eq!(color.read_u8(0x8000), 0x11);
    }

    #[test]
    fn mono_ram_hole_is_not_backed_and_reads_open_bus() {
        let mut mono = MemoryMap::new(Model::Mono, None);
        // 0x8000 is above the 16 KiB physical RAM on mono: writes drop, reads
        // return the model's open-bus value (unverified region, not fabricated).
        mono.write_u8(0x8000, 0x55);
        assert_eq!(mono.read_u8(0x8000), 0x90);
    }

    #[test]
    fn open_bus_value_is_model_dependent() {
        // 0x01A0: low 9 bits = 0x1A0 > 0xB7 → open bus (not the SoC $A0 alias).
        assert_eq!(MemoryMap::new(Model::Mono, None).io_read(0x01A0), 0x90);
        assert_eq!(MemoryMap::new(Model::Color, None).io_read(0x01A0), 0x00);
    }

    #[test]
    fn io_decodes_three_ways_including_the_soc_alias() {
        let map = MemoryMap::new(Model::Color, None);
        // $A0 SoC system control decodes (colour bit set).
        assert_eq!(map.io_read(0x00A0) & 0x02, 0x02);
        // SoC aliases every 512: 0x02A0 also hits system control...
        assert_eq!(map.io_read(0x02A0), map.io_read(0x00A0));
        // ...but 0x01A0 (low 9 bits 0x1A0 > 0xB7) is open bus, not SoC.
        assert_eq!(map.io_read(0x01A0), map.open_bus());
    }

    #[test]
    fn bank_registers_read_write_with_powerup_and_mask() {
        let rom = footer_rom(64 * 1024, [0xEA, 0, 0, 0, 0]);
        let cart = WsCartridge::from_bytes(&rom).unwrap();
        let mut map = MemoryMap::new(Model::Color, Some(cart));
        // $C3 powers up $FF.
        assert_eq!(map.io_read(io::REG_BANK_ROM1), 0xFF);
        // $C2 is full 8-bit.
        map.io_write(io::REG_BANK_ROM0, 0x5A);
        assert_eq!(map.io_read(io::REG_BANK_ROM0), 0x5A);
        // $C0 on a non-2003 cart masks readback to 4 bits.
        map.io_write(io::REG_BANK_LINEAR, 0xFF);
        assert_eq!(map.io_read(io::REG_BANK_LINEAR), 0x0F);
    }

    #[test]
    fn reset_vector_reaches_the_footer_for_every_rom_size() {
        // With the $C0 linear bank at its power-up $FF, physical 0xFFFF0 must map
        // to the top of ROM (the footer's boot far-jump) regardless of ROM size —
        // not just for the ≤ 1 MiB carts a $C0 of 0 would happen to reach.
        for size in [64 * 1024, 512 * 1024, 1024 * 1024, 4 * 1024 * 1024] {
            let rom = footer_rom(size, [0xEA, 0x11, 0x22, 0x33, 0x44]);
            let cart = WsCartridge::from_bytes(&rom).unwrap();
            let map = MemoryMap::new(Model::Color, Some(cart));
            assert_eq!(map.read_u8(0xF_FFF0), 0xEA, "boot opcode, size {size}");
            assert_eq!(map.read_u8(0xF_FFF1), 0x11, "operand, size {size}");
            assert_eq!(map.read_u8(0xF_FFF4), 0x44, "operand, size {size}");
        }
    }

    #[test]
    fn rom0_window_follows_its_bank_register() {
        let mut rom = footer_rom(128 * 1024, [0xEA, 0, 0, 0, 0]);
        rom[0x0_0000] = 0xA1; // bank 0, offset 0
        rom[0x1_0000] = 0xB2; // bank 1, offset 0
        let cart = WsCartridge::from_bytes(&rom).unwrap();
        let mut map = MemoryMap::new(Model::Color, Some(cart));
        map.io_write(io::REG_BANK_ROM0, 0x00);
        assert_eq!(map.read_u8(0x2_0000), 0xA1);
        map.io_write(io::REG_BANK_ROM0, 0x01);
        assert_eq!(map.read_u8(0x2_0000), 0xB2);
    }

    #[test]
    fn rom_region_is_read_only() {
        let rom = footer_rom(64 * 1024, [0xEA, 0, 0, 0, 0]);
        let cart = WsCartridge::from_bytes(&rom).unwrap();
        let mut map = MemoryMap::new(Model::Color, Some(cart));
        let before = map.read_u8(0x2_0000);
        map.write_u8(0x2_0000, before ^ 0xFF);
        assert_eq!(map.read_u8(0x2_0000), before, "ROM must ignore writes");
    }

    #[test]
    fn sram_window_round_trips_when_the_cart_declares_sram() {
        // Save-type code 0x02 → 32 KiB SRAM.
        let mut rom = footer_rom(64 * 1024, [0xEA, 0, 0, 0, 0]);
        rom[64 * 1024 - format_ws::HEADER_LEN + 0x0B] = 0x02;
        let cart = WsCartridge::from_bytes(&rom).unwrap();
        let mut map = MemoryMap::new(Model::Color, Some(cart));
        map.write_u8(0x1_0000, 0x7E);
        assert_eq!(map.read_u8(0x1_0000), 0x7E);
    }

    #[test]
    fn system_ctrl_lockout_is_one_way_and_reflects_bus_width() {
        // A 16-bit-bus cart (footer flags bit 2 set) shows $A0 bit 2 set (HLE).
        let mut rom = footer_rom(64 * 1024, [0xEA, 0, 0, 0, 0]);
        rom[64 * 1024 - format_ws::HEADER_LEN + 0x0C] = 0x04; // 16-bit bus
        let cart = WsCartridge::from_bytes(&rom).unwrap();
        let mut map = MemoryMap::new(Model::Color, Some(cart));
        assert_eq!(map.read_system_ctrl() & 0x04, 0x04, "16-bit bus reflected");

        // Set the lockout bit; it latches and cannot be cleared.
        map.io_write(io::REG_SYSTEM_CTRL, 0x01);
        assert_eq!(map.read_system_ctrl() & 0x01, 0x01);
        map.io_write(io::REG_SYSTEM_CTRL, 0x00);
        assert_eq!(map.read_system_ctrl() & 0x01, 0x01, "lockout is one-way");
    }
}
