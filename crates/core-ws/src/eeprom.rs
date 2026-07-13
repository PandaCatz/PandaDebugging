//! WonderSwan internal EEPROM (stores user settings: name, birthdate, blood
//! type, theme) with model-correct sizing.
//!
//! **Community bug #8 (deep-dive):** games detect WS vs WSC by probing the size
//! of the internal EEPROM — 512-bit on the mono WonderSwan, 16 Kbit (a 93C86) on
//! the Color and SwanCrystal. Emulators that always present one size break that
//! system detection. We size the device per model; on the smaller device the
//! high address bits are ignored (the storage aliases), which is exactly what a
//! detecting game observes.

/// The internal user-settings EEPROM, sized for the running system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalEeprom {
    data: Vec<u8>,
}

impl InternalEeprom {
    /// Mono WonderSwan: 512-bit (64-byte) internal EEPROM.
    pub const WS_BYTES: usize = 64;
    /// WonderSwan Color / SwanCrystal: 16 Kbit (2048-byte, 93C86) EEPROM.
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

    /// Community bug #8: WS and WSC present different EEPROM sizes.
    #[test]
    fn ws_and_wsc_sizes_differ() {
        assert_eq!(InternalEeprom::ws().size_bits(), 512);
        assert_eq!(InternalEeprom::wsc().size_bits(), 16 * 1024);
        assert_eq!(InternalEeprom::for_color(false).byte_len(), 64);
        assert_eq!(InternalEeprom::for_color(true).byte_len(), 2048);
    }

    /// The size is detectable: on the 64-byte WS device, address 64 aliases to
    /// 0; on the WSC device, address 64 is independent storage.
    #[test]
    fn size_is_detectable_by_address_aliasing() {
        let mut ws = InternalEeprom::ws();
        ws.write(0, 0xAA);
        assert_eq!(
            ws.read(InternalEeprom::WS_BYTES),
            0xAA,
            "addr 64 mirrors to 0 on the 64-byte WS device"
        );

        let mut wsc = InternalEeprom::wsc();
        wsc.write(0, 0xAA);
        wsc.write(InternalEeprom::WS_BYTES, 0x55);
        assert_eq!(wsc.read(0), 0xAA);
        assert_eq!(
            wsc.read(InternalEeprom::WS_BYTES),
            0x55,
            "WSC has real storage at address 64"
        );
    }
}
