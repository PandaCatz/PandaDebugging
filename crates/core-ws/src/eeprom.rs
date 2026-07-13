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
}
