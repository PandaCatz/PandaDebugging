#![forbid(unsafe_code)]

//! WonderSwan machine core — work in progress.
//!
//! Implemented today:
//! - [`cartridge`]: validated cartridge ownership boundary over a parsed image.
//! - [`io`]: the I/O register address map (doc-cited addresses; full map is
//!   transcribed against WSMan in Phase 0).
//! - [`interrupt`]: a behavioural model of the 8-line interrupt controller,
//!   including edge-vs-level semantics and priority ordering.
//! - [`machine`]: a minimal machine wiring the `cpu-v30mz` CPU to memory, the
//!   interrupt controller, and I/O, with hardware-IRQ delivery. Its memory map
//!   is a placeholder flat 1 MiB — the real WonderSwan map is future work.
//!
//! Not implemented yet: the real WonderSwan memory map (RAM sizing, ROM/SRAM
//! banking, full I/O), the PPU, the APU, and the DMA engines. This crate is not
//! a complete WonderSwan core.

pub mod apu;
pub mod cartridge;
pub mod interrupt;
pub mod io;
pub mod machine;

pub use apu::{NoiseChannel, NoiseLfsr};
pub use cartridge::{CartridgeError, WsCartridge};
pub use interrupt::{InterruptController, Irq, Trigger};
pub use machine::{Bus, Machine};
