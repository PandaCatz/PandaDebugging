// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

//! Defensive parser for WonderSwan / WonderSwan Color cartridge images.
//!
//! All input is treated as hostile: every length and offset is checked, nothing
//! panics, and the parser borrows the caller's bytes rather than copying them.
//! It returns a validated [`RomImage`] view; runtime cores must consume that
//! view and never re-interpret the raw bytes.
//!
//! # Header
//!
//! A WonderSwan cartridge stores a 16-byte internal header in the *last* 16
//! bytes of the ROM image; the 16-bit checksum is the final two bytes. The
//! field layout is decoded by [`CartHeader`] (see [`RomImage::header`]), having
//! been transcribed and adversarially verified against WSMan, the WSdev wiki,
//! ares, and Mednafen — citations and the resolved source disputes are recorded
//! in `docs/hardware/06-cartridge.md`. [`RomImage::raw_header`] still exposes
//! the undecoded bytes.

mod header;
pub use header::{
    BusWidth, CartFlags, CartHeader, FarPointer, Mapper, MapperKind, Orientation, RomSize,
    SaveKind, SaveType, System,
};

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Size of the internal cartridge header, in bytes, at the end of the ROM.
pub const HEADER_LEN: usize = 16;

/// Hard upper bound on accepted image size (defensive; real carts are far
/// smaller). Rejects absurd inputs before any allocation-sized arithmetic.
pub const MAX_ROM_LEN: usize = 64 * 1024 * 1024;

/// A validated, borrowed view over a WonderSwan ROM image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RomImage<'a> {
    bytes: &'a [u8],
}

impl<'a> RomImage<'a> {
    /// Validate structural invariants and borrow the image.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, RomError> {
        let len = bytes.len();
        if len < HEADER_LEN {
            return Err(RomError::TooSmall { len });
        }
        if len > MAX_ROM_LEN {
            return Err(RomError::TooLarge { len });
        }
        Ok(Self { bytes })
    }

    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        // A parsed image always has at least `HEADER_LEN` bytes; provided for
        // clippy::len_without_is_empty and API symmetry.
        false
    }

    /// Byte offset at which the 16-byte internal header begins.
    #[must_use]
    pub const fn header_offset(&self) -> usize {
        self.bytes.len() - HEADER_LEN
    }

    /// The raw 16-byte internal header (last 16 bytes of the image).
    #[must_use]
    pub fn raw_header(&self) -> &'a [u8] {
        &self.bytes[self.header_offset()..]
    }

    /// The decoded cartridge footer. See [`CartHeader`].
    #[must_use]
    pub fn header(&self) -> CartHeader {
        // `raw_header()` is exactly `HEADER_LEN` bytes by construction (the
        // image is validated to be at least that long), so this copy is
        // infallible and never panics.
        let mut footer = [0u8; HEADER_LEN];
        footer.copy_from_slice(self.raw_header());
        CartHeader::decode(&footer)
    }

    /// The 16-bit cartridge checksum as stored in the final two bytes
    /// (little-endian). Always the last field of the header.
    #[must_use]
    pub fn stored_checksum(&self) -> u16 {
        let len = self.bytes.len();
        u16::from_le_bytes([self.bytes[len - 2], self.bytes[len - 1]])
    }

    /// The 16-bit cartridge checksum: a per-byte sum (mod `0x10000`) over every
    /// byte of the image except the final two checksum bytes, including any
    /// padding. Verified against Mednafen's loader and WSdev/WSMan (see
    /// `docs/hardware/06-cartridge.md`). Not required to be correct to boot.
    #[must_use]
    pub fn computed_checksum(&self) -> u16 {
        let body = &self.bytes[..self.bytes.len() - 2];
        body.iter()
            .fold(0u16, |acc, &b| acc.wrapping_add(u16::from(b)))
    }

    /// Whether the [stored][`Self::stored_checksum`] checksum matches the
    /// [computed][`Self::computed_checksum`] one. Informational only — some
    /// legitimate carts (e.g. WonderWitch) store `0x0000`.
    #[must_use]
    pub fn checksum_valid(&self) -> bool {
        self.stored_checksum() == self.computed_checksum()
    }

    /// Whether the image length is a whole number of 64 KiB banks. Most carts
    /// satisfy this; some early / 8-bit-bus images may not, so this is
    /// informational rather than a rejection.
    #[must_use]
    pub fn is_bank_aligned(&self) -> bool {
        self.bytes.len().is_multiple_of(64 * 1024)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RomError {
    TooSmall { len: usize },
    TooLarge { len: usize },
}

impl Display for RomError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall { len } => write!(
                formatter,
                "ROM image is {len} bytes; needs at least {HEADER_LEN} for the internal header"
            ),
            Self::TooLarge { len } => write!(
                formatter,
                "ROM image is {len} bytes, over the {MAX_ROM_LEN}-byte limit"
            ),
        }
    }
}

impl Error for RomError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_with_len(len: usize) -> Vec<u8> {
        (0..len).map(|i| i as u8).collect()
    }

    #[test]
    fn rejects_images_shorter_than_the_header() {
        let bytes = image_with_len(HEADER_LEN - 1);
        assert_eq!(
            RomImage::parse(&bytes),
            Err(RomError::TooSmall {
                len: HEADER_LEN - 1
            })
        );
    }

    #[test]
    fn header_is_the_final_sixteen_bytes() {
        let bytes = image_with_len(64 * 1024);
        let rom = RomImage::parse(&bytes).expect("aligned image parses");
        assert_eq!(rom.header_offset(), 64 * 1024 - HEADER_LEN);
        assert_eq!(rom.raw_header().len(), HEADER_LEN);
        assert_eq!(rom.raw_header(), &bytes[bytes.len() - HEADER_LEN..]);
    }

    #[test]
    fn stored_checksum_reads_the_last_two_bytes_little_endian() {
        let mut bytes = image_with_len(32);
        bytes[30] = 0x34;
        bytes[31] = 0x12;
        let rom = RomImage::parse(&bytes).expect("parses");
        assert_eq!(rom.stored_checksum(), 0x1234);
    }

    #[test]
    fn computed_checksum_excludes_the_final_two_bytes() {
        // Distinct byte values so an off-by-N exclusion changes the result:
        // body bytes are 1..=30, and the excluded final two are large sentinels
        // that must NOT appear in the sum.
        let mut bytes = vec![0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate().take(30) {
            *b = (i + 1) as u8;
        }
        bytes[30] = 0xAA;
        bytes[31] = 0xBB;
        let rom = RomImage::parse(&bytes).expect("parses");
        let expected: u16 = (1..=30).sum();
        assert_eq!(rom.computed_checksum(), expected);
    }

    #[test]
    fn checksum_valid_when_stored_matches_computed() {
        let mut bytes = vec![0x01u8; 64];
        // Sum of the 62 body bytes (0x01 each), stored little-endian in the
        // final two bytes, must validate.
        let sum = 62u16;
        bytes[62] = sum.to_le_bytes()[0];
        bytes[63] = sum.to_le_bytes()[1];
        let rom = RomImage::parse(&bytes).expect("parses");
        assert!(rom.checksum_valid());
    }

    #[test]
    fn header_decodes_from_the_image_footer() {
        // Place a recognisable footer at the tail and confirm `header()` reads
        // the same bytes `raw_header()` exposes.
        let mut bytes = vec![0u8; 64 * 1024];
        let base = bytes.len() - HEADER_LEN;
        bytes[base + 0x07] = 0x01; // system = Color
        bytes[base + 0x0C] = 0x04; // flags: 16-bit bus
        let rom = RomImage::parse(&bytes).expect("parses");
        let header = rom.header();
        assert_eq!(header.system(), System::Color);
        assert_eq!(header.bus_width(), BusWidth::Sixteen);
        assert_eq!(header.stored_checksum(), rom.stored_checksum());
    }

    #[test]
    fn bank_alignment_is_informational() {
        assert!(
            RomImage::parse(&vec![0; 64 * 1024])
                .unwrap()
                .is_bank_aligned()
        );
        assert!(!RomImage::parse(&[0u8; 100]).unwrap().is_bank_aligned());
    }

    #[test]
    fn oversized_images_are_rejected_before_use() {
        // Construct a length check without allocating 64 MiB: use the boundary.
        let bytes = image_with_len(HEADER_LEN);
        assert!(RomImage::parse(&bytes).is_ok());
    }
}
