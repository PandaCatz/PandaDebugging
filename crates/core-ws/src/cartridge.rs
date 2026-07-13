//! Owned, validated cartridge boundary.
//!
//! Mirrors the project rule that runtime state owns *validated* data and never
//! re-parses raw images. [`WsCartridge`] takes a [`format_ws::RomImage`] view
//! that has already passed structural validation and copies out exactly the
//! bytes the machine will run.

use format_ws::{RomError, RomImage};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A WonderSwan cartridge the core owns for the lifetime of a session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WsCartridge {
    rom: Vec<u8>,
    header: [u8; format_ws::HEADER_LEN],
    stored_checksum: u16,
}

impl WsCartridge {
    /// Take ownership of an already-validated image view.
    pub fn from_image(image: RomImage<'_>) -> Result<Self, CartridgeError> {
        let header: [u8; format_ws::HEADER_LEN] = image
            .raw_header()
            .try_into()
            .map_err(|_| CartridgeError::HeaderLength)?;
        Ok(Self {
            rom: image.bytes().to_vec(),
            header,
            stored_checksum: image.stored_checksum(),
        })
    }

    /// Validate raw bytes and take ownership in one step.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CartridgeError> {
        let image = RomImage::parse(bytes).map_err(CartridgeError::Rom)?;
        Self::from_image(image)
    }

    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    #[must_use]
    pub const fn raw_header(&self) -> &[u8; format_ws::HEADER_LEN] {
        &self.header
    }

    #[must_use]
    pub const fn stored_checksum(&self) -> u16 {
        self.stored_checksum
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CartridgeError {
    Rom(RomError),
    /// The validated image did not yield a 16-byte header. Unreachable given the
    /// [`RomImage`] invariants, but represented rather than unwrapped.
    HeaderLength,
}

impl Display for CartridgeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rom(err) => write!(formatter, "invalid ROM image: {err}"),
            Self::HeaderLength => formatter.write_str("cartridge header was not 16 bytes"),
        }
    }
}

impl Error for CartridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Rom(err) => Some(err),
            Self::HeaderLength => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_validated_bytes_and_header() {
        let mut bytes = vec![0u8; 64 * 1024];
        bytes[64 * 1024 - 2] = 0xAD;
        bytes[64 * 1024 - 1] = 0xDE;
        let cart = WsCartridge::from_bytes(&bytes).expect("valid image");
        assert_eq!(cart.rom().len(), 64 * 1024);
        assert_eq!(cart.raw_header().len(), format_ws::HEADER_LEN);
        assert_eq!(cart.stored_checksum(), 0xDEAD);
    }

    #[test]
    fn rejects_undersized_images_at_the_boundary() {
        let bytes = vec![0u8; 4];
        assert!(matches!(
            WsCartridge::from_bytes(&bytes),
            Err(CartridgeError::Rom(_))
        ));
    }
}
