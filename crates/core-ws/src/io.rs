// SPDX-License-Identifier: GPL-3.0-or-later
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

// Sound (channel-4 noise). Verified vs WSdev Sound / libws / ares / Mednafen
// (see docs/hardware/07-io-registers.md).

/// `SND_NOISE` (`$8E`): channel-4 noise control. Bits 0-2 = tap mode; bit 3
/// (`0x08`) = LFSR reset (write-1 strobe, reads 0); bit 4 (`0x10`) = noise/LFSR
/// update enable; bits 5-7 unused.
pub const REG_SND_NOISE: u16 = 0x08E;

/// `SND_CTRL` (`$90`): bits 0-3 = channel 1-4 enable (ch4 = bit 3, `0x08`); bit 7
/// (`0x80`) = channel-4 output select (`0` = wavetable, `1` = noise).
pub const REG_SND_CTRL: u16 = 0x090;

/// `SND_RANDOM` (`$92` lo / `$93` hi): the live 15-bit noise LFSR, read-only
/// (`$92` = bits 0-7, `$93` = bits 8-14; bit 15 reads 0).
pub const REG_SND_RANDOM_LO: u16 = 0x092;
pub const REG_SND_RANDOM_HI: u16 = 0x093;

/// General-purpose DMA registers. Available only in WSC colour mode.
pub const REG_DMA_START: u16 = 0x040;
pub const REG_DMA_END: u16 = 0x048;

/// Sound DMA (SDMA) registers feeding the PCM voice channel. 24-bit length.
pub const REG_SDMA_START: u16 = 0x04A;
pub const REG_SDMA_END: u16 = 0x052;

/// System control / hardware flags (`$A0`, `SYSTEM_CTRL1`): boot-ROM lockout
/// (bit 0, one-way latch), colour-system status (bit 1, read), external ROM bus
/// width (bit 2, `0`=8-bit/`1`=16-bit), ROM access speed (bit 3), cartridge/
/// self-test OK (bit 7, read). See `docs/hardware/01-cpu-v30mz.md` §6.
pub const REG_SYSTEM_CTRL: u16 = 0x0A0;

/// Interrupt vector base: the hardware IVT may live anywhere in the first 64 KiB.
pub const REG_INT_BASE: u16 = 0x0B0;

/// Standard cartridge mapper bank-select registers (the cartridge I/O block).
/// `$C0` linear/EX bank (4 bits on Bandai 2001, 6 on 2003), `$C1` SRAM bank,
/// `$C2` ROM bank 0, `$C3` ROM bank 1 (powers up `$FF`). See §2.
pub const REG_BANK_LINEAR: u16 = 0x0C0;
pub const REG_BANK_SRAM: u16 = 0x0C1;
pub const REG_BANK_ROM0: u16 = 0x0C2;
pub const REG_BANK_ROM1: u16 = 0x0C3;

/// UART data (`$B1`): read = RX byte, write = TX byte. Verified vs WSMan/WSdev.
pub const REG_SER_DATA: u16 = 0x0B1;

/// UART status (read) / control (write) (`$B3`): bit 7 (`0x80`) = enable, bit 6 =
/// baud, bit 5 (write) = overrun-reset, bit 2 (read) = TX-ready, bit 1 (read) =
/// overrun, bit 0 (read) = RX-ready. Verified vs WSMan/WSdev/libws/ares.
pub const REG_SER_STATUS: u16 = 0x0B3;

/// Keypad matrix read. Unattached matrix lines read 0 (pull-down); some games
/// refuse to boot if unmapped bits read 1.
pub const REG_KEYPAD: u16 = 0x0B5;

// Internal EEPROM ($B8-$BF block). Verified vs libws/WSdev/ares (canonical
// high-nibble strobe layout on $BE; two documented wrong layouts were refuted).
// NOT yet wired: `InternalEeprom` needs the Microwire command state machine —
// see docs/hardware/07-io-registers.md for the protocol.

/// Internal-EEPROM 16-bit data buffer (`$BA` lo / `$BB` hi).
pub const REG_IEEP_DATA_LO: u16 = 0x0BA;
pub const REG_IEEP_DATA_HI: u16 = 0x0BB;
/// Internal-EEPROM Microwire command word (`$BC` lo / `$BD` hi): start + 2-bit
/// opcode + address.
pub const REG_IEEP_CMD_LO: u16 = 0x0BC;
pub const REG_IEEP_CMD_HI: u16 = 0x0BD;
/// Internal-EEPROM control (write) / status (read) (`$BE`). Write strobes: READ
/// `0x10`, WRITE `0x20`, SHORT/ERASE/EWEN/EWDS `0x40`, PROTECT `0x80`. Status:
/// bit 1 (`0x02`) = ready/idle, bit 0 (`0x01`) = read-done.
pub const REG_IEEP_CTRL: u16 = 0x0BE;

/// Interrupt acknowledge (write): clears edge-triggered pending lines only.
pub const REG_INT_ACK: u16 = 0x0B6;

/// Cartridge RTC status (read) / command (write) for the Seiko S-3511A protocol.
pub const REG_RTC_STATUS: u16 = 0x0CA;
/// Cartridge RTC data port.
pub const REG_RTC_DATA: u16 = 0x0CB;
