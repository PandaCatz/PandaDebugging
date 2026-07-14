// SPDX-License-Identifier: GPL-3.0-or-later
//! WonderSwan internal EEPROM (stores user settings: name, birthdate, blood
//! type, theme) with model-correct sizing.
//!
//! **Community bug #8:** the internal EEPROM is a different size per model —
//! **1 Kbit (128 bytes, a 93LC46-class part) on the mono WonderSwan**, and
//! **16 Kbit (2048 bytes, a 93LC86) on the Color and SwanCrystal**. Emulators
//! that present one fixed size corrupt the settings block and break software
//! that relies on the real capacity / Microwire address-field width. (Verified
//! against ares `ws/system/system.cpp`, which allocates 128 vs 2048, and the
//! WSdev Internal EEPROM page — the "64-byte" figure in the deep-dive summary
//! was wrong.)
//!
//! Note: WS-vs-WSC *system* detection is done via the colour/system-control
//! register, not by size-probing the EEPROM; we size it correctly because BIOS
//! and software depend on the real capacity, not as a detection primitive. On
//! the smaller device the high address bits are ignored (the storage aliases),
//! matching the Microwire address-field width.

/// The internal user-settings EEPROM, sized for the running system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalEeprom {
    data: Vec<u8>,
}

impl InternalEeprom {
    /// Mono WonderSwan: 1 Kbit (128-byte) internal EEPROM.
    pub const WS_BYTES: usize = 128;
    /// WonderSwan Color / SwanCrystal: 16 Kbit (2048-byte, 93LC86) EEPROM.
    pub const WSC_BYTES: usize = 2048;

    #[must_use]
    pub fn ws() -> Self {
        Self {
            data: vec![0; Self::WS_BYTES],
        }
    }

    #[must_use]
    pub fn wsc() -> Self {
        Self {
            data: vec![0; Self::WSC_BYTES],
        }
    }

    /// Build for the running system (`true` = Color / SwanCrystal, `false` = mono WS).
    #[must_use]
    pub fn for_color(is_color: bool) -> Self {
        if is_color { Self::wsc() } else { Self::ws() }
    }

    /// Capacity in bytes (64 for WS, 2048 for WSC).
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.data.len()
    }

    /// Capacity in bits — the value a detecting game infers (512 vs 16384).
    #[must_use]
    pub fn size_bits(&self) -> usize {
        self.data.len() * 8
    }

    /// Read a byte. Address bits beyond the device size are ignored (the storage
    /// aliases), which is the basis of the WS-vs-WSC size probe.
    #[must_use]
    pub fn read(&self, address: usize) -> u8 {
        self.data[address % self.data.len()]
    }

    /// Write a byte, with the same address aliasing as [`read`](Self::read).
    pub fn write(&mut self, address: usize, value: u8) {
        let len = self.data.len();
        self.data[address % len] = value;
    }
}

/// The internal-EEPROM register interface (`$BA`–`$BE`): the settings store plus
/// the address and command latches.
///
/// Models the **register-window** behaviour shared by Mednafen, BizHawk, and
/// Cygne — the data ports are a live window into the store at the current word
/// address, and `$BE` returns synthetic ready/done status (operations complete
/// instantly). This delivers community bug #8 (WS-vs-WSC size detection) through
/// the model-sized address aliasing of [`InternalEeprom`].
///
/// Not modelled: the Microwire write-protect / EWEN-EWDS command protocol (the
/// ares-accurate behaviour). It is a separate accuracy concern, not part of the
/// size-detection bug — see `docs/hardware/07-io-registers.md`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalEepromPort {
    eeprom: InternalEeprom,
    /// Word address latched from `$BC` (low) / `$BD` (high).
    address: u16,
    /// Last `$BE` command byte (read back as synthetic status).
    command: u8,
}

impl InternalEepromPort {
    #[must_use]
    pub fn new(is_color: bool) -> Self {
        Self {
            eeprom: InternalEeprom::for_color(is_color),
            address: 0,
            command: 0,
        }
    }

    /// Byte offset for the current word address (`high` selects the high byte).
    /// The store aliases model-sized, which is what a game probes for the size.
    const fn byte_addr(&self, high: bool) -> usize {
        ((self.address as usize) << 1) | (high as usize)
    }

    /// Read a data byte: `$BA` (low) / `$BB` (high) at the current word address.
    #[must_use]
    pub fn read_data(&self, high: bool) -> u8 {
        self.eeprom.read(self.byte_addr(high))
    }

    /// Write a data byte: `$BA` (low) / `$BB` (high) at the current word address.
    pub fn write_data(&mut self, high: bool, value: u8) {
        let addr = self.byte_addr(high);
        self.eeprom.write(addr, value);
    }

    /// Set the word-address low (`$BC`) or high (`$BD`) byte.
    pub const fn set_addr(&mut self, high: bool, value: u8) {
        if high {
            self.address = (self.address & 0x00FF) | ((value as u16) << 8);
        } else {
            self.address = (self.address & 0xFF00) | value as u16;
        }
    }

    /// Address register readback: `$BC` (low) / `$BD` (high).
    #[must_use]
    pub const fn addr_byte(&self, high: bool) -> u8 {
        if high {
            (self.address >> 8) as u8
        } else {
            self.address as u8
        }
    }

    /// Latch a `$BE` command byte.
    pub const fn set_command(&mut self, value: u8) {
        self.command = value;
    }

    /// The `$BE` status read: synthetic ready (bit 1) / read-done (bit 0), since
    /// operations complete instantly (Mednafen/BizHawk/Cygne behaviour).
    #[must_use]
    pub const fn status(&self) -> u8 {
        if self.command & 0x20 != 0 {
            self.command | 0x02
        } else if self.command & 0x10 != 0 {
            self.command | 0x01
        } else {
            self.command | 0x03
        }
    }

    /// The backing settings store (for save/state and tests).
    #[must_use]
    pub const fn eeprom(&self) -> &InternalEeprom {
        &self.eeprom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Community bug #8: WS (1 Kbit) and WSC (16 Kbit) EEPROM sizes differ.
    #[test]
    fn ws_and_wsc_sizes_differ() {
        assert_eq!(InternalEeprom::ws().size_bits(), 1024);
        assert_eq!(InternalEeprom::wsc().size_bits(), 16 * 1024);
        assert_eq!(InternalEeprom::for_color(false).byte_len(), 128);
        assert_eq!(InternalEeprom::for_color(true).byte_len(), 2048);
    }

    /// High address bits alias on the smaller device: on the 128-byte WS part,
    /// address 128 wraps to 0; on the WSC part it is independent storage.
    #[test]
    fn address_aliases_on_the_smaller_device() {
        let mut ws = InternalEeprom::ws();
        ws.write(0, 0xAA);
        assert_eq!(
            ws.read(InternalEeprom::WS_BYTES),
            0xAA,
            "addr 128 aliases to 0 on the 128-byte WS device"
        );

        let mut wsc = InternalEeprom::wsc();
        wsc.write(0, 0xAA);
        wsc.write(InternalEeprom::WS_BYTES, 0x55);
        assert_eq!(wsc.read(0), 0xAA);
        assert_eq!(
            wsc.read(InternalEeprom::WS_BYTES),
            0x55,
            "WSC has real storage at address 128"
        );
    }

    #[test]
    fn port_data_window_round_trips_at_the_addressed_word() {
        let mut port = InternalEepromPort::new(true); // WSC
        port.set_addr(false, 0x05); // word address 5
        port.set_addr(true, 0x00);
        port.write_data(false, 0xAA); // $BA
        port.write_data(true, 0x55); // $BB
        assert_eq!(port.read_data(false), 0xAA);
        assert_eq!(port.read_data(true), 0x55);
        assert_eq!(port.addr_byte(false), 0x05);
    }

    #[test]
    fn port_status_reports_synthetic_ready_done() {
        let mut port = InternalEepromPort::new(false);
        port.set_command(0x20); // WRITE strobe -> ready (bit 1)
        assert_ne!(port.status() & 0x02, 0);
        port.set_command(0x10); // READ strobe -> read-done (bit 0)
        assert_ne!(port.status() & 0x01, 0);
        port.set_command(0x00); // idle -> both set
        assert_eq!(port.status() & 0x03, 0x03);
    }

    /// Community bug #8 through the register interface: the small (WS) device
    /// aliases, so a probe past its size reads earlier data; the large (WSC)
    /// device has independent storage there.
    #[test]
    fn size_detection_via_address_aliasing_through_the_ports() {
        // WS: 128 bytes = 64 words. Word 64 -> byte 128 -> aliases to word 0.
        let mut ws = InternalEepromPort::new(false);
        ws.write_data(false, 0x11); // word 0
        ws.set_addr(false, 64);
        assert_eq!(ws.read_data(false), 0x11, "WS aliases word 64 to word 0");

        // WSC: 2048 bytes = 1024 words. Word 64 is independent storage.
        let mut wsc = InternalEepromPort::new(true);
        wsc.write_data(false, 0x11); // word 0
        wsc.set_addr(false, 64);
        wsc.write_data(false, 0x22); // word 64
        wsc.set_addr(false, 0);
        assert_eq!(wsc.read_data(false), 0x11, "WSC word 0 is unchanged");
    }
}
