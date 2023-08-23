pub mod datatypes;
pub mod frame;
pub mod message;
pub mod ping;

/// Offline message marker.
pub(super) const MAGIC: [u8; 16] = [
    0x00, 0xFF, 0xFF, 0x00, 0xFE, 0xFE, 0xFE, 0xFE, 0xFD, 0xFD, 0xFD, 0xFD, 0x12, 0x34, 0x56, 0x78,
];

/// Supported Raknet Protocol versions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolVersion {
    Unsupported(u8),
    V10,
    V11,
}

impl ProtocolVersion {
    pub fn from_u8(version: u8) -> Self {
        match version {
            10 => Self::V10,
            11 => Self::V11,
            version => Self::Unsupported(version),
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            Self::Unsupported(version) => *version,
            Self::V10 => 10,
            Self::V11 => 11,
        }
    }
}
