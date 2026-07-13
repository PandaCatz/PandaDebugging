//! Behavioural model of the WonderSwan interrupt controller.
//!
//! Eight hardware lines share one enable/status/ack register triple plus a
//! relocatable vector base (`REG_INT_BASE`). Two facts trip up most emulators
//! and are modelled explicitly here:
//!
//! 1. **Priority is by bit position** — the highest set-and-enabled bit wins, so
//!    `HblankTimer` (bit 7) outranks `Vblank` (bit 6), down to `SerialTx`
//!    (bit 0). Getting this ordering wrong desynchronises audio/raster games.
//! 2. **Some lines are level-triggered, not edge-triggered.** Level lines stay
//!    asserted until their *source* condition clears; writing `REG_INT_ACK`
//!    clears only the edge lines. Treating every line as edge-triggered
//!    deadlocks any game that leans on the serial TX/RX interrupts.
//!
//! This model is CPU-independent and fully unit-tested; the bus layer will drive
//! it (sources call [`InterruptController::raise`]/[`lower`], the CPU reads
//! [`pending_enabled`] and the register accessors).
//!
//! [`lower`]: InterruptController::lower
//! [`pending_enabled`]: InterruptController::pending_enabled

/// The eight WonderSwan hardware interrupt lines.
///
/// The discriminant is both the register bit position and the priority rank:
/// a higher value is higher priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum Irq {
    SerialTx = 0,
    Keypad = 1,
    Cartridge = 2,
    SerialRx = 3,
    LineCompare = 4,
    VblankTimer = 5,
    Vblank = 6,
    HblankTimer = 7,
}

impl Irq {
    /// All lines, lowest priority first.
    pub const ALL: [Irq; 8] = [
        Irq::SerialTx,
        Irq::Keypad,
        Irq::Cartridge,
        Irq::SerialRx,
        Irq::LineCompare,
        Irq::VblankTimer,
        Irq::Vblank,
        Irq::HblankTimer,
    ];

    /// This line's single-bit mask within the enable/status/ack registers.
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// Priority rank; higher wins when several lines are pending and enabled.
    #[must_use]
    pub const fn priority(self) -> u8 {
        self as u8
    }

    /// Whether this line is edge- or level-triggered on real hardware.
    #[must_use]
    pub const fn trigger(self) -> Trigger {
        match self {
            Irq::SerialTx | Irq::Cartridge | Irq::SerialRx => Trigger::Level,
            Irq::Keypad | Irq::LineCompare | Irq::VblankTimer | Irq::Vblank | Irq::HblankTimer => {
                Trigger::Edge
            }
        }
    }

    /// The line occupying bit `index` (0..=7), if any.
    #[must_use]
    pub const fn from_bit_index(index: u8) -> Option<Irq> {
        match index {
            0 => Some(Irq::SerialTx),
            1 => Some(Irq::Keypad),
            2 => Some(Irq::Cartridge),
            3 => Some(Irq::SerialRx),
            4 => Some(Irq::LineCompare),
            5 => Some(Irq::VblankTimer),
            6 => Some(Irq::Vblank),
            7 => Some(Irq::HblankTimer),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    Edge,
    Level,
}

/// Mask of the edge-triggered lines — the only bits `REG_INT_ACK` may clear.
pub const EDGE_MASK: u8 = Irq::Keypad.bit()
    | Irq::LineCompare.bit()
    | Irq::VblankTimer.bit()
    | Irq::Vblank.bit()
    | Irq::HblankTimer.bit();

/// Mask of the level-triggered lines.
pub const LEVEL_MASK: u8 = Irq::SerialTx.bit() | Irq::Cartridge.bit() | Irq::SerialRx.bit();

/// The `REG_INT_BASE`/`REG_INT_ENABLE`/`REG_INT_STATUS`/`REG_INT_ACK` state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InterruptController {
    base: u8,
    enable: u8,
    pending: u8,
}

impl InterruptController {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            base: 0,
            enable: 0,
            pending: 0,
        }
    }

    /// `REG_INT_BASE` write.
    pub const fn set_base(&mut self, base: u8) {
        self.base = base;
    }

    #[must_use]
    pub const fn base(&self) -> u8 {
        self.base
    }

    /// `REG_INT_ENABLE` write.
    pub const fn set_enable(&mut self, mask: u8) {
        self.enable = mask;
    }

    #[must_use]
    pub const fn enable(&self) -> u8 {
        self.enable
    }

    /// `REG_INT_STATUS` read: the raw pending mask.
    #[must_use]
    pub const fn status(&self) -> u8 {
        self.pending
    }

    /// A source asserts its line. On hardware the **enable** bit gates whether
    /// the status latch is set, so a line raised while it is *disabled* is not
    /// latched and will not fire on a later enable. (A subsequent disable does
    /// not clear an already-latched bit — the gate is at set time only.)
    pub const fn raise(&mut self, irq: Irq) {
        if self.enable & irq.bit() != 0 {
            self.pending |= irq.bit();
        }
    }

    /// A level-triggered source's condition cleared. No-op for edge lines (they
    /// are cleared by [`acknowledge`](Self::acknowledge)).
    pub const fn lower(&mut self, irq: Irq) {
        if let Trigger::Level = irq.trigger() {
            self.pending &= !irq.bit();
        }
    }

    /// `REG_INT_ACK` write. Clears only the edge-triggered pending bits named in
    /// `mask`; level lines are untouched and remain asserted until their source
    /// resolves.
    pub const fn acknowledge(&mut self, mask: u8) {
        self.pending &= !(mask & EDGE_MASK);
    }

    /// Highest-priority line that is both pending and enabled, if any.
    #[must_use]
    pub const fn pending_enabled(&self) -> Option<Irq> {
        let active = self.pending & self.enable;
        if active == 0 {
            return None;
        }
        let top = 7 - (active.leading_zeros() as u8);
        Irq::from_bit_index(top)
    }

    /// Interrupt vector number: the base's high 5 bits with the line number in
    /// the low 3 bits — `(base & 0xF8) | line`, **not** `base + line`. Hardware
    /// replaces bits 2..0 with the highest pending line index.
    #[must_use]
    pub const fn vector(&self, irq: Irq) -> u8 {
        (self.base & 0xF8) | (irq as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_and_level_masks_partition_all_lines() {
        assert_eq!(EDGE_MASK | LEVEL_MASK, 0xFF);
        assert_eq!(EDGE_MASK & LEVEL_MASK, 0x00);
    }

    #[test]
    fn highest_bit_wins_priority() {
        let mut ic = InterruptController::new();
        ic.set_enable(0xFF);
        ic.raise(Irq::Vblank);
        ic.raise(Irq::HblankTimer);
        assert_eq!(ic.pending_enabled(), Some(Irq::HblankTimer));
    }

    #[test]
    fn disabled_lines_are_never_selected() {
        let mut ic = InterruptController::new();
        ic.set_enable(Irq::Vblank.bit()); // HblankTimer disabled
        ic.raise(Irq::HblankTimer);
        ic.raise(Irq::Vblank);
        assert_eq!(ic.pending_enabled(), Some(Irq::Vblank));
    }

    #[test]
    fn ack_clears_edge_lines_but_not_level_lines() {
        let mut ic = InterruptController::new();
        ic.set_enable(0xFF);
        ic.raise(Irq::Keypad); // edge
        ic.raise(Irq::Cartridge); // level
        ic.acknowledge(0xFF);
        assert_eq!(ic.status() & Irq::Keypad.bit(), 0, "edge line acknowledged");
        assert_ne!(
            ic.status() & Irq::Cartridge.bit(),
            0,
            "level line survives ack"
        );
    }

    #[test]
    fn lower_only_affects_level_lines() {
        let mut ic = InterruptController::new();
        ic.set_enable(0xFF); // raise is enable-gated
        ic.raise(Irq::Cartridge); // level
        ic.raise(Irq::Vblank); // edge
        ic.lower(Irq::Cartridge);
        ic.lower(Irq::Vblank);
        assert_eq!(
            ic.status() & Irq::Cartridge.bit(),
            0,
            "level source cleared"
        );
        assert_ne!(
            ic.status() & Irq::Vblank.bit(),
            0,
            "edge line unaffected by lower"
        );
    }

    #[test]
    fn raise_is_gated_by_enable() {
        let mut ic = InterruptController::new(); // enable = 0
        ic.raise(Irq::Vblank);
        assert_eq!(ic.status(), 0, "raise while disabled does not latch");
        ic.set_enable(Irq::Vblank.bit());
        assert_eq!(
            ic.pending_enabled(),
            None,
            "enabling later does not fire the stale line"
        );
        ic.raise(Irq::Vblank);
        assert_eq!(
            ic.pending_enabled(),
            Some(Irq::Vblank),
            "raise while enabled latches"
        );
    }

    #[test]
    fn vector_masks_base_low_three_bits() {
        let mut ic = InterruptController::new();
        ic.set_base(0x40); // 8-aligned
        assert_eq!(ic.vector(Irq::Vblank), 0x46);
        // A base with dirty low bits: the line number REPLACES bits 2..0.
        ic.set_base(0x1D);
        assert_eq!(ic.vector(Irq::Vblank), 0x1E, "(0x1D & 0xF8) | 6");
        assert_eq!(ic.vector(Irq::SerialTx), 0x18, "(0x1D & 0xF8) | 0");
    }

    #[test]
    fn nothing_pending_selects_nothing() {
        let ic = InterruptController::new();
        assert_eq!(ic.pending_enabled(), None);
    }
}
