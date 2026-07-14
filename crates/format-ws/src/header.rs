//! Decoded WonderSwan / WonderSwan Color cartridge footer.
//!
//! The internal header occupies the **last 16 bytes** of the ROM image. Because
//! the final ROM bank maps to the CPU reset vector (`FFFF:0000`), footer byte
//! `0x00` is physical `0xFFFF0` — so the first five footer bytes are the boot
//! far-jump the V30MZ executes at power-on.
//!
//! The field layout was transcribed field-by-field and adversarially verified
//! against WSMan, the WSdev wiki, ares, and Mednafen. Citations, the resolved
//! source disputes (bus-width bit, save-code `0x01` size), and the remaining
//! open gaps are recorded in `docs/hardware/06-cartridge.md`. Decoding is total:
//! every undocumented code becomes an explicit `Other`/`Unknown`/`None` rather
//! than a guess or a panic.

use crate::HEADER_LEN;

// Field offsets within the 16-byte footer.
const OFF_BOOT: usize = 0x00; // 5-byte far JMP at the reset vector
const OFF_MAINTENANCE: usize = 0x05;
const OFF_PUBLISHER: usize = 0x06;
const OFF_SYSTEM: usize = 0x07;
const OFF_GAME_ID: usize = 0x08;
const OFF_VERSION: usize = 0x09;
const OFF_ROM_SIZE: usize = 0x0A;
const OFF_SAVE_TYPE: usize = 0x0B;
const OFF_FLAGS: usize = 0x0C;
const OFF_MAPPER: usize = 0x0D;
const OFF_CHECKSUM: usize = 0x0E;

/// A `segment:offset` far pointer, as stored in the boot far-jump.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FarPointer {
    pub segment: u16,
    pub offset: u16,
}

/// Console family the cartridge declares (footer `0x07`).
///
/// `0 = WonderSwan (mono)`, `1 = WonderSwan Color`. The boot ROM reportedly
/// ignores this byte and some carts clear it incorrectly, so it is advisory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum System {
    Mono,
    Color,
    Other(u8),
}

impl System {
    const fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Mono,
            1 => Self::Color,
            other => Self::Other(other),
        }
    }

    /// Colour-capable. Follows ares, which treats any non-`Mono` value as
    /// colour-capable rather than requiring exactly `1`.
    #[must_use]
    pub const fn is_color(self) -> bool {
        !matches!(self, Self::Mono)
    }
}

/// Declared ROM capacity (footer `0x0A`).
///
/// Emulators derive the real size from the file length; this is only the
/// cartridge's declared code. Codes `0x00`/`0x01` are inferred and `0x0A`/`0x0B`
/// are WSdev-only extensions (see `06-cartridge.md`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RomSize {
    code: u8,
}

impl RomSize {
    #[must_use]
    pub const fn code(self) -> u8 {
        self.code
    }

    /// Declared capacity in bytes, or `None` for a code no source documents.
    #[must_use]
    pub const fn bytes(self) -> Option<u32> {
        Some(match self.code {
            0x00 => 128 * 1024,
            0x01 => 256 * 1024,
            0x02 => 512 * 1024,
            0x03 => 1024 * 1024,
            0x04 => 2 * 1024 * 1024,
            0x05 => 3 * 1024 * 1024,
            0x06 => 4 * 1024 * 1024,
            0x07 => 6 * 1024 * 1024,
            0x08 => 8 * 1024 * 1024,
            0x09 => 16 * 1024 * 1024,
            0x0A => 32 * 1024 * 1024,
            0x0B => 64 * 1024 * 1024,
            _ => return None,
        })
    }
}

/// Kind of cartridge backing store selected by the save-type code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveKind {
    None,
    Sram,
    Eeprom,
    Unknown,
}

/// Declared save-memory type and size (footer `0x0B`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveType {
    code: u8,
}

impl SaveType {
    #[must_use]
    pub const fn code(self) -> u8 {
        self.code
    }

    #[must_use]
    pub const fn kind(self) -> SaveKind {
        match self.code {
            0x00 => SaveKind::None,
            0x01..=0x05 => SaveKind::Sram,
            0x10 | 0x20 | 0x50 => SaveKind::Eeprom,
            _ => SaveKind::Unknown,
        }
    }

    /// Backing-store capacity in bytes (`0x00` = none → `Some(0)`), or `None`
    /// for an undocumented code.
    #[must_use]
    pub const fn bytes(self) -> Option<u32> {
        Some(match self.code {
            0x00 => 0,
            // Historically documented as 8 KB, but every known 0x01 cartridge
            // ships a 256 Kbit (32 KiB) SRAM chip (WSdev, ares). Allocating only
            // 8 KB would corrupt saves on those carts. See 06-cartridge.md.
            0x01 => 32 * 1024,
            0x02 => 32 * 1024,
            0x03 => 128 * 1024,
            0x04 => 256 * 1024,
            0x05 => 512 * 1024,
            0x10 => 128,
            0x20 => 2048,
            0x50 => 1024,
            _ => return None,
        })
    }
}

/// Display orientation (footer `0x0C` bit 0).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// External ROM bus width (footer `0x0C` bit 2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusWidth {
    Eight,
    Sixteen,
}

/// Cartridge configuration flags (footer `0x0C`). Distinct from the runtime I/O
/// register `REG_HW_FLAGS` (port `$A0`), though bits 2–3 mirror it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CartFlags {
    raw: u8,
}

impl CartFlags {
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.raw
    }

    /// Display orientation — bit 0. Unanimous across every source.
    #[must_use]
    pub const fn orientation(self) -> Orientation {
        if self.raw & 0x01 == 0 {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        }
    }

    /// External ROM bus width — bit 2, `0 = 8-bit`, `1 = 16-bit`. Resolves
    /// community bug #9. This is the best-evidence reading (ares
    /// `metadata[12] & 4`, the WSdev raw diagram, and WSMan's own
    /// `REG_HW_FLAGS`); WSMan's cartridge-metadata table instead places it at
    /// bit 1 with inverted polarity — see `06-cartridge.md`.
    #[must_use]
    pub const fn bus_width(self) -> BusWidth {
        if self.raw & 0x04 == 0 {
            BusWidth::Eight
        } else {
            BusWidth::Sixteen
        }
    }

    /// ROM access-speed bit — bit 3. Sources disagree on whether it counts
    /// cycles or wait states and on polarity; timing is deferred, so the raw
    /// bit is exposed without committing to a cycle interpretation.
    #[must_use]
    pub const fn rom_speed_bit(self) -> bool {
        self.raw & 0x08 != 0
    }
}

/// Cartridge mapper / RTC nibble (footer `0x0D`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapperKind {
    Bandai2001,
    Bandai2003,
    Other(u8),
}

/// The mapper byte (footer `0x0D`). The low nibble selects the mapper; the high
/// nibble must be zero. The RTC-bearing variant is Bandai 2003.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mapper {
    raw: u8,
}

impl Mapper {
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.raw
    }

    #[must_use]
    pub const fn kind(self) -> MapperKind {
        match self.raw & 0x0F {
            0x00 => MapperKind::Bandai2001,
            0x01 => MapperKind::Bandai2003,
            other => MapperKind::Other(other),
        }
    }

    /// Whether a cartridge RTC is present — the Bandai 2003 mapper (nibble
    /// `0x01`). Matches ares's `metadata[13] == 1`.
    #[must_use]
    pub const fn has_rtc(self) -> bool {
        self.raw & 0x0F == 0x01
    }
}

/// The fully-decoded 16-byte cartridge footer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CartHeader {
    boot: [u8; 5],
    maintenance: u8,
    publisher_id: u8,
    system: System,
    game_id: u8,
    version: u8,
    rom_size: RomSize,
    save_type: SaveType,
    flags: CartFlags,
    mapper: Mapper,
    stored_checksum: u16,
}

impl CartHeader {
    /// Decode the 16-byte footer. Infallible: undocumented codes become
    /// explicit `Other`/`Unknown`/`None`, so hostile input never panics.
    #[must_use]
    pub const fn decode(footer: &[u8; HEADER_LEN]) -> Self {
        Self {
            boot: [
                footer[OFF_BOOT],
                footer[OFF_BOOT + 1],
                footer[OFF_BOOT + 2],
                footer[OFF_BOOT + 3],
                footer[OFF_BOOT + 4],
            ],
            maintenance: footer[OFF_MAINTENANCE],
            publisher_id: footer[OFF_PUBLISHER],
            system: System::from_byte(footer[OFF_SYSTEM]),
            game_id: footer[OFF_GAME_ID],
            version: footer[OFF_VERSION],
            rom_size: RomSize {
                code: footer[OFF_ROM_SIZE],
            },
            save_type: SaveType {
                code: footer[OFF_SAVE_TYPE],
            },
            flags: CartFlags {
                raw: footer[OFF_FLAGS],
            },
            mapper: Mapper {
                raw: footer[OFF_MAPPER],
            },
            stored_checksum: u16::from_le_bytes([footer[OFF_CHECKSUM], footer[OFF_CHECKSUM + 1]]),
        }
    }

    /// The boot far-jump target at the reset vector, if the first footer byte is
    /// the x86 / V30MZ `JMP FAR ptr16:16` opcode (`0xEA`).
    #[must_use]
    pub const fn boot_entry(self) -> Option<FarPointer> {
        if self.boot[0] != 0xEA {
            return None;
        }
        Some(FarPointer {
            offset: u16::from_le_bytes([self.boot[1], self.boot[2]]),
            segment: u16::from_le_bytes([self.boot[3], self.boot[4]]),
        })
    }

    #[must_use]
    pub const fn maintenance(self) -> u8 {
        self.maintenance
    }
    #[must_use]
    pub const fn publisher_id(self) -> u8 {
        self.publisher_id
    }
    #[must_use]
    pub const fn system(self) -> System {
        self.system
    }
    #[must_use]
    pub const fn game_id(self) -> u8 {
        self.game_id
    }
    #[must_use]
    pub const fn version(self) -> u8 {
        self.version
    }
    #[must_use]
    pub const fn rom_size(self) -> RomSize {
        self.rom_size
    }
    #[must_use]
    pub const fn save_type(self) -> SaveType {
        self.save_type
    }
    #[must_use]
    pub const fn flags(self) -> CartFlags {
        self.flags
    }
    #[must_use]
    pub const fn mapper(self) -> Mapper {
        self.mapper
    }
    /// External ROM bus width — convenience for `flags().bus_width()` (bug #9).
    #[must_use]
    pub const fn bus_width(self) -> BusWidth {
        self.flags.bus_width()
    }
    /// The 16-bit checksum stored in the final two footer bytes (little-endian).
    #[must_use]
    pub const fn stored_checksum(self) -> u16 {
        self.stored_checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 16-byte footer with all the documented fields set to distinct,
    /// recognisable values.
    fn sample_footer() -> [u8; HEADER_LEN] {
        [
            0xEA, 0x00, 0x40, 0x00, 0xF0, // boot: JMP FAR F000:4000
            0x00, // maintenance
            0x01, // publisher = Bandai
            0x01, // system = Color
            0x2A, // game id
            0x03, // version
            0x04, // rom size code -> 2 MiB
            0x02, // save type -> 32 KiB SRAM
            0x0D, // flags: b0 vertical, b2 16-bit bus, b3 speed
            0x01, // mapper: Bandai 2003 (RTC)
            0x34, 0x12, // checksum = 0x1234 LE
        ]
    }

    #[test]
    fn decodes_every_documented_field() {
        let h = CartHeader::decode(&sample_footer());
        assert_eq!(h.publisher_id(), 0x01);
        assert_eq!(h.system(), System::Color);
        assert!(h.system().is_color());
        assert_eq!(h.game_id(), 0x2A);
        assert_eq!(h.version(), 0x03);
        assert_eq!(h.rom_size().code(), 0x04);
        assert_eq!(h.rom_size().bytes(), Some(2 * 1024 * 1024));
        assert_eq!(h.save_type().kind(), SaveKind::Sram);
        assert_eq!(h.save_type().bytes(), Some(32 * 1024));
        assert_eq!(h.flags().orientation(), Orientation::Vertical);
        assert_eq!(h.bus_width(), BusWidth::Sixteen);
        assert!(h.flags().rom_speed_bit());
        assert_eq!(h.mapper().kind(), MapperKind::Bandai2003);
        assert!(h.mapper().has_rtc());
        assert_eq!(h.stored_checksum(), 0x1234);
        assert_eq!(
            h.boot_entry(),
            Some(FarPointer {
                segment: 0xF000,
                offset: 0x4000
            })
        );
    }

    #[test]
    fn bus_width_bit2_selects_8_or_16_bit() {
        // Community bug #9: bit 2 clear = 8-bit (Pocket Challenge V2 / early
        // carts), set = 16-bit. Everything else in the flags byte is irrelevant.
        let mut footer = sample_footer();
        footer[0x0C] = 0x00; // bit2 = 0
        assert_eq!(
            CartHeader::decode(&footer).bus_width(),
            BusWidth::Eight,
            "bit 2 clear must decode as 8-bit"
        );
        footer[0x0C] = 0x04; // bit2 = 1
        assert_eq!(
            CartHeader::decode(&footer).bus_width(),
            BusWidth::Sixteen,
            "bit 2 set must decode as 16-bit"
        );
        // The orientation bit must not bleed into the bus-width reading, nor
        // vice-versa.
        footer[0x0C] = 0x01; // b0 set, b2 clear
        let h = CartHeader::decode(&footer);
        assert_eq!(h.bus_width(), BusWidth::Eight);
        assert_eq!(h.flags().orientation(), Orientation::Vertical);
    }

    #[test]
    fn zero_flags_and_system_decode_as_the_low_values() {
        // Pins the "off" polarity of every flag and the mono/no-RTC baseline —
        // the values a wrong mask or inverted comparison would silently flip.
        let h = CartHeader::decode(&[0u8; HEADER_LEN]);
        assert_eq!(h.system(), System::Mono);
        assert!(!h.system().is_color());
        assert_eq!(h.flags().orientation(), Orientation::Horizontal);
        assert_eq!(h.bus_width(), BusWidth::Eight);
        assert!(!h.flags().rom_speed_bit());
        assert_eq!(h.mapper().kind(), MapperKind::Bandai2001);
        assert!(!h.mapper().has_rtc());
    }

    #[test]
    fn save_code_01_is_32_kib_not_the_stale_8_kb() {
        // Verified correction: real 0x01 carts carry 256 Kbit (32 KiB) chips.
        assert_eq!(SaveType { code: 0x01 }.bytes(), Some(32 * 1024));
        assert_eq!(SaveType { code: 0x01 }.kind(), SaveKind::Sram);
    }

    #[test]
    fn rom_size_table_maps_known_codes_and_flags_unknowns() {
        assert_eq!(RomSize { code: 0x02 }.bytes(), Some(512 * 1024));
        assert_eq!(RomSize { code: 0x09 }.bytes(), Some(16 * 1024 * 1024));
        assert_eq!(RomSize { code: 0x0B }.bytes(), Some(64 * 1024 * 1024));
        assert_eq!(RomSize { code: 0xFF }.bytes(), None);
    }

    #[test]
    fn undocumented_codes_are_explicit_never_panics() {
        let mut footer = [0u8; HEADER_LEN];
        footer[OFF_SYSTEM] = 0x77;
        footer[OFF_ROM_SIZE] = 0xEE;
        footer[OFF_SAVE_TYPE] = 0xEE;
        footer[OFF_MAPPER] = 0x07;
        let h = CartHeader::decode(&footer);
        assert_eq!(h.system(), System::Other(0x77));
        assert_eq!(h.rom_size().bytes(), None);
        assert_eq!(h.save_type().kind(), SaveKind::Unknown);
        assert_eq!(h.save_type().bytes(), None);
        assert_eq!(h.mapper().kind(), MapperKind::Other(0x07));
        assert!(!h.mapper().has_rtc());
        // A non-0xEA first byte means no decodable boot vector.
        assert_eq!(h.boot_entry(), None);
    }
}
