//! WonderSwan I/O register address map.
//!
//! Only addresses explicitly cited in the hardware deep-dive are declared as
//! hard constants here. The complete `$000–$0FF` map — including the many
//! entries marked unknown/TODO in WSMan — is transcribed field-by-field in
//! Phase 0 before the bus wires them up. Adding an unverified address is worse
//! than a gap, so gaps are left explicit.
//!
//! Reference: WSMan (<http://daifukkat.su/docs/wsman/>).

/// Total displayed + blanked scanline count. Writable by games; setting it to
/// 255 blanks the display, and below 144 stops VBlank IRQs. On SwanCrystal an
/// odd value physically damages the LCD (model as corrupted scanlines).
pub const REG_LCD_VTOTAL: u16 = 0x016;

/// Mono shade pool: four registers defining an 8-entry, 4-bit shade pool that
/// the 16 mono palettes index into (`$0` brightest .. `$F` darkest).
pub const REG_PALMONO_POOL_START: u16 = 0x01C;
pub const REG_PALMONO_POOL_END: u16 = 0x01F;

/// Mono palettes: 16 palettes × 4 colours, each colour a 3-bit index into the
/// shade pool above.
pub const REG_PALMONO_START: u16 = 0x020;
pub const REG_PALMONO_END: u16 = 0x03F;

/// General-purpose DMA registers. Available only in WSC colour mode.
pub const REG_DMA_START: u16 = 0x040;
pub const REG_DMA_END: u16 = 0x048;

/// Sound DMA (SDMA) registers feeding the PCM voice channel. 24-bit length.
pub const REG_SDMA_START: u16 = 0x04A;
pub const REG_SDMA_END: u16 = 0x052;

/// Interrupt vector base: the hardware IVT may live anywhere in the first 64 KiB.
pub const REG_INT_BASE: u16 = 0x0B0;

/// Keypad matrix read. Unattached matrix lines read 0 (pull-down); some games
/// refuse to boot if unmapped bits read 1.
pub const REG_KEYPAD: u16 = 0x0B5;

/// Interrupt acknowledge (write): clears edge-triggered pending lines only.
pub const REG_INT_ACK: u16 = 0x0B6;

/// Cartridge RTC status (read) / command (write) for the Seiko S-3511A protocol.
pub const REG_RTC_STATUS: u16 = 0x0CA;
/// Cartridge RTC data port.
pub const REG_RTC_DATA: u16 = 0x0CB;
