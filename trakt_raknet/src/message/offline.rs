use std::net::SocketAddr;

use crate::datatypes::{ReadBuf, WriteBuf};
use crate::ProtocolVersion;

use super::{write_header, Message, MessageError, RaknetMessage};

#[derive(Clone, Debug)]
pub struct MessageUnconnectedPing {
    pub client_uuid: i64,
    pub forward_timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct MessageOpenConnectionRequest1 {
    pub raknet_protocol: ProtocolVersion,
    pub mtu_size: u16,
}

#[derive(Clone, Debug)]
pub struct MessageOpenConnectionReply1 {
    pub server_uuid: i64,
    pub use_encryption: bool,
    pub preferred_mtu_size: u16,
}

#[derive(Clone, Debug)]
pub struct MessageOpenConnectionRequest2 {
    pub client_uuid: i64,
    pub server_address: SocketAddr,
    pub preferred_mtu_size: u16,
}

#[derive(Clone, Debug)]
pub struct MessageOpenConnectionReply2 {
    pub server_uuid: i64,
    pub client_address: SocketAddr,
    pub use_encryption: bool,
    pub mtu_size: u16,
}

#[derive(Clone, Debug)]
pub struct MessageAlreadyConnected {
    pub server_uuid: i64,
}

#[derive(Clone, Debug)]
pub struct MessageIncompatibleProtocolVersion {
    pub server_uuid: i64,
    pub preferred_protocol: ProtocolVersion,
}

#[derive(Clone, Debug)]
pub struct MessageUnconnectedPong {
    pub timestamp: i64,
    pub server_uuid: i64,
    pub motd: String,
}

impl Message for MessageUnconnectedPing {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::UnconnectedPing)?;
        buf.write_i64(self.forward_timestamp)?;
        buf.write_magic()?;
        buf.write_i64(self.client_uuid)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let timestamp = buf.read_i64()?;
        buf.read_magic()?;
        Ok(Self {
            forward_timestamp: timestamp,
            client_uuid: buf.read_i64()?,
        })
    }
}

impl Message for MessageOpenConnectionRequest1 {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::OpenConnectionRequest1)?;
        buf.write_magic()?;
        buf.write_u8(self.raknet_protocol.to_u8())?;
        let mtu_bytes = vec![0; buf.0.len() + 28];
        buf.0.extend_from_slice(&mtu_bytes);
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let buf_size = buf.0.len() + 1; // consider removed byte from packet id
        buf.read_magic()?;
        Ok(Self {
            raknet_protocol: ProtocolVersion::from_u8(buf.read_u8()?),
            mtu_size: (buf_size + 28)
                .try_into()
                .map_err(|_| MessageError::MTUInvalidPadding)?,
        })
    }
}

impl Message for MessageOpenConnectionReply1 {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::OpenConnectionReply1)?;
        buf.write_magic()?;
        buf.write_i64(self.server_uuid)?;
        buf.write_bool(self.use_encryption)?;
        buf.write_u16(self.preferred_mtu_size)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        buf.read_magic()?;
        Ok(Self {
            server_uuid: buf.read_i64()?,
            use_encryption: buf.read_bool()?,
            preferred_mtu_size: buf.read_u16()?,
        })
    }
}

impl Message for MessageOpenConnectionRequest2 {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::OpenConnectionRequest2)?;
        buf.write_magic()?;
        buf.write_address(self.server_address)?;
        buf.write_u16(self.preferred_mtu_size)?;
        buf.write_i64(self.client_uuid)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        buf.read_magic()?;
        Ok(Self {
            server_address: buf.read_address()?,
            preferred_mtu_size: buf.read_u16()?,
            client_uuid: buf.read_i64()?,
        })
    }
}

impl Message for MessageOpenConnectionReply2 {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::OpenConnectionReply2)?;
        buf.write_magic()?;
        buf.write_i64(self.server_uuid)?;
        buf.write_address(self.client_address)?;
        buf.write_u16(self.mtu_size)?;
        buf.write_bool(self.use_encryption)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        buf.read_magic()?;
        Ok(Self {
            server_uuid: buf.read_i64()?,
            client_address: buf.read_address()?,
            mtu_size: buf.read_u16()?,
            use_encryption: buf.read_bool()?,
        })
    }
}

impl Message for MessageAlreadyConnected {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::AlreadyConnected)?;
        buf.write_magic()?;
        buf.write_i64(self.server_uuid)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        buf.read_magic()?;
        Ok(Self {
            server_uuid: buf.read_i64()?,
        })
    }
}

impl Message for MessageIncompatibleProtocolVersion {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::IncompatibleProtocolVersion)?;
        buf.write_u8(self.preferred_protocol.to_u8())?;
        buf.write_magic()?;
        buf.write_i64(self.server_uuid)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let preferred_protocol = ProtocolVersion::from_u8(buf.read_u8()?);
        buf.read_magic()?;
        let server_uuid = buf.read_i64()?;
        Ok(Self {
            server_uuid,
            preferred_protocol,
        })
    }
}

impl Message for MessageUnconnectedPong {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::UnconnectedPong)?;
        buf.write_i64(self.timestamp)?;
        buf.write_i64(self.server_uuid)?;
        buf.write_magic()?;
        buf.write_str(&self.motd)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let timestamp = buf.read_i64()?;
        let server_uuid = buf.read_i64()?;
        buf.read_magic()?;
        let motd = buf.read_str()?;
        Ok(Self {
            timestamp,
            server_uuid,
            motd,
        })
    }
}
