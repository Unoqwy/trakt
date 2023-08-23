mod offline;
mod online;

pub use offline::*;
pub use online::*;

use super::datatypes::{BufError, ReadBuf, WriteBuf};

#[derive(Clone, Debug)]
pub enum MessageError {
    /// MTU padding is invalid, it couldn't be deserialized
    MTUInvalidPadding,
    /// Error while serializing/deserializing the message
    BufError(BufError),
    /// Frame is invalid
    InvalidFrame,
    /// Reliability unknown
    UnknownRealibility(u8),
    /// Message was empty, there was nothing to unpack
    ZeroSize,
}

pub trait Message: Sized {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError>;

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError>;

    fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        let mut buf = WriteBuf::new();
        self.serialize(&mut buf)?;
        Ok(buf.0.to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaknetMessage {
    ConnectedPing,
    UnconnectedPing,
    UnconnectedPingOpenConnections,
    ConnectedPong,
    DetectLostConnection,
    OpenConnectionRequest1,
    OpenConnectionReply1,
    OpenConnectionRequest2,
    OpenConnectionReply2,
    ConnectionRequest,
    ConnectionRequestAccepted = 0x10,
    ConnectionRequestFailed = 0x11,
    AlreadyConnected = 0x12,
    NewIncomingConnection = 0x13,
    NoFreeIncomingConnection = 0x14,
    DisconnectNotification = 0x15,
    ConnectionLost = 0x16,
    ConnectionBanned = 0x17,
    IncompatibleProtocolVersion = 0x19,
    UnconnectedPong = 0x1c,
}

impl RaknetMessage {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::ConnectedPing),
            0x01 => Some(Self::UnconnectedPing),
            0x02 => Some(Self::UnconnectedPingOpenConnections),
            0x03 => Some(Self::ConnectedPong),
            0x04 => Some(Self::DetectLostConnection),
            0x05 => Some(Self::OpenConnectionRequest1),
            0x06 => Some(Self::OpenConnectionReply1),
            0x07 => Some(Self::OpenConnectionRequest2),
            0x08 => Some(Self::OpenConnectionReply2),
            0x09 => Some(Self::ConnectionRequest),
            0x10 => Some(Self::ConnectionRequestAccepted),
            0x11 => Some(Self::ConnectionRequestFailed),
            0x12 => Some(Self::AlreadyConnected),
            0x13 => Some(Self::NewIncomingConnection),
            0x14 => Some(Self::NoFreeIncomingConnection),
            0x15 => Some(Self::DisconnectNotification),
            0x16 => Some(Self::ConnectionLost),
            0x17 => Some(Self::ConnectionBanned),
            0x19 => Some(Self::IncompatibleProtocolVersion),
            0x1c => Some(Self::UnconnectedPong),
            _ => None,
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            Self::ConnectedPing => 0x00,
            Self::UnconnectedPing => 0x01,
            Self::UnconnectedPingOpenConnections => 0x02,
            Self::ConnectedPong => 0x03,
            Self::DetectLostConnection => 0x04,
            Self::OpenConnectionRequest1 => 0x05,
            Self::OpenConnectionReply1 => 0x06,
            Self::OpenConnectionRequest2 => 0x07,
            Self::OpenConnectionReply2 => 0x08,
            Self::ConnectionRequest => 0x09,
            Self::ConnectionRequestAccepted => 0x10,
            Self::ConnectionRequestFailed => 0x11,
            Self::AlreadyConnected => 0x12,
            Self::NewIncomingConnection => 0x13,
            Self::NoFreeIncomingConnection => 0x14,
            Self::DisconnectNotification => 0x15,
            Self::ConnectionLost => 0x16,
            Self::ConnectionBanned => 0x17,
            Self::IncompatibleProtocolVersion => 0x19,
            Self::UnconnectedPong => 0x1c,
        }
    }
}

impl From<BufError> for MessageError {
    fn from(err: BufError) -> Self {
        Self::BufError(err)
    }
}

impl From<MessageError> for anyhow::Error {
    fn from(value: MessageError) -> Self {
        Self::msg(format!("{:?}", value))
    }
}

#[inline]
pub(super) fn write_header(
    buf: &mut WriteBuf,
    packet_type: RaknetMessage,
) -> Result<(), MessageError> {
    buf.write_u8(packet_type.to_u8())?;
    Ok(())
}
