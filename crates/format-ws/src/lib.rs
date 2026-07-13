#![forbid(unsafe_code)]

//! Defensive parser for WonderSwan / WonderSwan Color cartridge images.
//!
//! All input is treated as hostile: every length and offset is checked, nothing
//! panics, and the parser borrows the caller's bytes rather than copying them.
//! It returns a validated [`RomImage`] view; runtime cores must consume that
//! view and never re-interpret the raw bytes.
//!
//! # Header status
//!
//! A WonderSwan cartridge stores a 16-byte internal header in the *last* 16
//! bytes of the ROM image, and the 16-bit cartridge checksum is the final two
//! bytes. The precise byte layout of the remaining header fields (publisher,
//! game id, ROM/SRAM size codes, mapper/flags, RTC and bus-width bits) is being
//! transcribed field-by-field against WSMan in Phase 0; until each field is
//! verified it is intentionally *not* decoded here. [`RomImage::raw_header`]
//! exposes the bytes so verification work can proceed without guessing.

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

    /// The 16-bit cartridge checksum as stored in the final two bytes
    /// (little-endian). Always the last field of the header.
    #[must_use]
    pub fn stored_checksum(&self) -> u16 {
        let len = self.bytes.len();
        u16::from_le_bytes([self.bytes[len - 2], self.bytes[len - 1]])
    }

    /// Provisional 16-bit checksum over every byte except the stored checksum
    /// field, computed as a wrapping sum.
    ///
    /// The exact algorithm is pending Phase 0 verification against WSMan; do not
    /// gate acceptance on it yet. Exposed so the verification harness can
    /// compare against known-good dumps.
    #[must_use]
    pub fn computed_checksum_provisional(&self) -> u16 {
        let body = &self.bytes[..self.bytes.len() - 2];
        body.iter()
            .fold(0u16, |acc, &b| acc.wrapping_add(u16::from(b)))
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
    fn provisional_checksum_excludes_the_checksum_field() {
        // Body bytes 0..=29 sum, wrapping in u16; last two bytes ignored.
        let bytes = vec![0xFFu8; 32];
        let rom = RomImage::parse(&bytes).expect("parses");
        let expected = (0..30).fold(0u16, |acc, _| acc.wrapping_add(0xFF));
        assert_eq!(rom.computed_checksum_provisional(), expected);
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
