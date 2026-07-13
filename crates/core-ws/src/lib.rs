#![forbid(unsafe_code)]

//! WonderSwan machine core — work in progress.
//!
//! Implemented today (all independently testable, no CPU required):
//! - [`cartridge`]: validated cartridge ownership boundary over a parsed image.
//! - [`io`]: the I/O register address map (doc-cited addresses; full map is
//!   transcribed against WSMan in Phase 0).
//! - [`interrupt`]: a behavioural model of the 8-line interrupt controller,
//!   including edge-vs-level semantics and priority ordering.
//!
//! Not implemented yet (Phase 2+): the NEC V30MZ CPU, the memory bus and access
//! slots, the PPU, the APU, and the general-purpose / sound DMA engines. This
//! crate must not be described as a running WonderSwan core.

pub mod cartridge;
pub mod interrupt;
pub mod io;

pub use cartridge::{CartridgeError, WsCartridge};
pub use interrupt::{InterruptController, Irq, Trigger};
