// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

//! WonderSwan machine core — work in progress.
//!
//! Implemented today:
//! - [`cartridge`]: validated cartridge ownership boundary over a parsed image.
//! - [`io`]: the I/O register address map (doc-cited addresses; full map is
//!   transcribed against WSMan in Phase 0).
//! - [`interrupt`]: a behavioural model of the 8-line interrupt controller,
//!   including edge-vs-level semantics and priority ordering.
//! - [`memory`]: the real WonderSwan address-routing map — internal RAM sized by
//!   model, the cartridge ROM/SRAM bank windows (`$C0`–`$C3`), the I/O three-way
//!   decode, `$A0` system control, and model-dependent open-bus reads.
//! - [`machine`]: a minimal machine wiring the `cpu-v30mz` CPU to the memory map,
//!   the interrupt controller, and I/O, with hardware-IRQ delivery. It can boot
//!   from cartridge ROM via the reset vector.
//!
//! Not implemented yet: the boot-ROM overlay, the internal-EEPROM/RTC register
//! protocols, the full SoC register file, the PPU, the APU, the DMA engines, and
//! cycle timing. This crate is not a complete WonderSwan core.

pub mod apu;
pub mod cartridge;
pub mod eeprom;
pub mod interrupt;
pub mod io;
pub mod machine;
pub mod memory;
pub mod palette;
pub mod ppu;
pub mod serial;

pub use apu::{NoiseChannel, NoiseLfsr};
pub use cartridge::{CartridgeError, WsCartridge};
pub use eeprom::InternalEeprom;
pub use interrupt::{InterruptController, Irq, Trigger};
pub use machine::{Bus, Machine};
pub use memory::MemoryMap;
pub use palette::{Depth, MonoPalettes, color_zero_transparent};
pub use ppu::SpriteUnit;
pub use serial::Serial;
