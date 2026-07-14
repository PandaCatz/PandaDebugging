// SPDX-License-Identifier: GPL-3.0-or-later
//! WonderSwan serial port (UART) — the EXT link-cable / WonderWitch interface.
//!
//! **Community bug #3 (deep-dive):** disabling the UART must clear its pending
//! TX/RX interrupt lines. Both `HWINT_SER_TX` and `HWINT_SER_RX` are
//! *level-triggered*, so if an emulator leaves them asserted after the port is
//! disabled, a game that toggles the serial port during init receives spurious
//! IRQs and can lock up. Fixed upstream in ares v144; we get it right here by
//! lowering the level lines when the port goes from enabled to disabled.

use crate::interrupt::{InterruptController, Irq};

/// Serial-enable bit in `REG_SER_STATUS` (bit 7, per WSMan/libws). The exact
/// register *address* is wired in with the verified I/O map; the enable-bit
/// semantics and the interrupt behaviour are what this module pins down.
pub const SER_ENABLE: u8 = 1 << 7;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Serial {
    enabled: bool,
}

impl Serial {
    #[must_use]
    pub const fn new() -> Self {
        Self { enabled: false }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The transmitter is ready: assert `HWINT_SER_TX` while the port is enabled.
    pub const fn signal_tx_ready(&self, interrupts: &mut InterruptController) {
        if self.enabled {
            interrupts.raise(Irq::SerialTx);
        }
    }

    /// A byte arrived: assert `HWINT_SER_RX` while the port is enabled.
    pub const fn signal_rx_ready(&self, interrupts: &mut InterruptController) {
        if self.enabled {
            interrupts.raise(Irq::SerialRx);
        }
    }

    /// Enable or disable the port. **Disabling clears the pending TX/RX IRQ
    /// lines** (community bug #3): both are level-triggered, so the source must
    /// stop asserting them, otherwise they fire spuriously and can hang a game.
    pub const fn set_enabled(&mut self, enabled: bool, interrupts: &mut InterruptController) {
        if self.enabled && !enabled {
            interrupts.lower(Irq::SerialTx);
            interrupts.lower(Irq::SerialRx);
        }
        self.enabled = enabled;
    }

    /// Write `REG_SER_STATUS`; bit 7 ([`SER_ENABLE`]) drives the enable state.
    pub const fn write_status(&mut self, value: u8, interrupts: &mut InterruptController) {
        self.set_enabled(value & SER_ENABLE != 0, interrupts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Community bug #3: disabling the UART clears pending TX/RX IRQs.
    #[test]
    fn disabling_clears_pending_serial_irqs() {
        let mut ic = InterruptController::new();
        ic.set_enable(Irq::SerialTx.bit() | Irq::SerialRx.bit()); // raise is enable-gated
        let mut serial = Serial::new();
        serial.set_enabled(true, &mut ic);
        serial.signal_tx_ready(&mut ic);
        serial.signal_rx_ready(&mut ic);
        assert_ne!(ic.status() & Irq::SerialTx.bit(), 0);
        assert_ne!(ic.status() & Irq::SerialRx.bit(), 0);

        serial.set_enabled(false, &mut ic);
        assert_eq!(
            ic.status() & Irq::SerialTx.bit(),
            0,
            "SER_TX must clear on disable"
        );
        assert_eq!(
            ic.status() & Irq::SerialRx.bit(),
            0,
            "SER_RX must clear on disable"
        );
    }

    #[test]
    fn disabled_port_raises_nothing() {
        let mut ic = InterruptController::new();
        let serial = Serial::new(); // starts disabled
        serial.signal_tx_ready(&mut ic);
        serial.signal_rx_ready(&mut ic);
        assert_eq!(ic.status(), 0);
    }

    #[test]
    fn write_status_bit7_drives_enable_and_the_fix() {
        let mut ic = InterruptController::new();
        ic.set_enable(Irq::SerialRx.bit()); // raise is enable-gated
        let mut serial = Serial::new();
        serial.write_status(SER_ENABLE, &mut ic);
        assert!(serial.is_enabled());
        serial.signal_rx_ready(&mut ic);
        assert_ne!(ic.status() & Irq::SerialRx.bit(), 0);

        serial.write_status(0x00, &mut ic); // clear enable bit
        assert!(!serial.is_enabled());
        assert_eq!(ic.status() & Irq::SerialRx.bit(), 0, "RX IRQ cleared");
    }
}
