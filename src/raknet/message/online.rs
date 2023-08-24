#![allow(clippy::comparison_chain)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bytes::Buf;

use crate::raknet::datatypes::{BufError, ReadBuf, WriteBuf};

use super::{write_header, Message, MessageError, RaknetMessage};

#[derive(Clone, Debug)]
pub struct MessageConnectedPing {
    pub timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct MessageConnectedPong {
    pub ping_timestamp: i64,
    pub pong_timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct MessageConnectionRequest {
    pub client_uuid: i64,
    pub forward_timestamp: i64,
    pub use_encryption: bool,
}

#[derive(Clone, Debug)]
pub struct MessageConnectionRequestAccepted {
    pub client_address: SocketAddr,
    pub request_timestamp: i64,
    pub accept_timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct MessageNewIncomingConnection {
    pub server_address: SocketAddr,
    pub request_timestamp: i64,
    pub accept_timestamp: i64,
}

impl Message for MessageConnectedPing {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::ConnectedPing)?;
        buf.write_i64(self.timestamp)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        Ok(Self {
            timestamp: buf.read_i64()?,
        })
    }
}

impl Message for MessageConnectedPong {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::ConnectedPong)?;
        buf.write_i64(self.ping_timestamp)?;
        buf.write_i64(self.pong_timestamp)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        Ok(Self {
            ping_timestamp: buf.read_i64()?,
            pong_timestamp: buf.read_i64()?,
        })
    }
}

impl Message for MessageConnectionRequest {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::ConnectionRequest)?;
        buf.write_i64(self.client_uuid)?;
        buf.write_i64(self.forward_timestamp)?;
        buf.write_bool(self.use_encryption)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        Ok(Self {
            client_uuid: buf.read_i64()?,
            forward_timestamp: buf.read_i64()?,
            use_encryption: buf.read_bool()?,
        })
    }
}

impl Message for MessageConnectionRequestAccepted {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::ConnectionRequestAccepted)?;
        buf.write_address(self.client_address)?;
        buf.write_u16(0)?; // system index
        let tmp_address = get_bogus_system_address();
        for _ in 0..10 {
            buf.write_address(tmp_address)?;
        }
        buf.write_i64(self.request_timestamp)?;
        buf.write_i64(self.accept_timestamp)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let client_address = buf.read_address()?;
        buf.0.advance(2); // system index
        loop {
            let _ = buf.read_address()?;
            let remaining = buf.0.remaining();
            if remaining == 16 {
                break;
            } else if remaining < 16 {
                return Err(BufError::InvalidAdrress.into());
            }
        }
        Ok(Self {
            client_address,
            request_timestamp: buf.read_i64()?,
            accept_timestamp: buf.read_i64()?,
        })
    }
}

impl Message for MessageNewIncomingConnection {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        write_header(buf, RaknetMessage::NewIncomingConnection)?;
        buf.write_address(self.server_address)?;
        let tmp_address = get_bogus_system_address();
        for _ in 0..10 {
            buf.write_address(tmp_address)?;
        }
        buf.write_i64(self.request_timestamp)?;
        buf.write_i64(self.accept_timestamp)?;
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let server_address = buf.read_address()?;
        loop {
            let _ = buf.read_address()?;
            let remaining = buf.0.remaining();
            if remaining == 16 {
                break;
            } else if remaining < 16 {
                return Err(BufError::InvalidAdrress.into());
            }
        }
        Ok(Self {
            server_address,
            request_timestamp: buf.read_i64()?,
            accept_timestamp: buf.read_i64()?,
        })
    }
}

fn get_bogus_system_address() -> SocketAddr {
    let tmp_ipv4 = Ipv4Addr::new(255, 255, 255, 255);
    SocketAddr::new(IpAddr::V4(tmp_ipv4), 19132)
}
