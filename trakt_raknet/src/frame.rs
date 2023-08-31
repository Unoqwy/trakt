use bytes::Buf;

use super::datatypes::{ReadBuf, WriteBuf};
use super::message::{Message, MessageError};

const FLAG_FRAGMENTED: u8 = 0x10;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Reliability {
    Unreliable,
    UnreliableSequenced,
    Reliable,
    ReliableOrdered,
    ReliableSequenced,
}

impl Reliability {
    pub fn is_reliable(&self) -> bool {
        matches!(
            self,
            Self::Reliable | Self::ReliableOrdered | Self::ReliableSequenced
        )
    }

    pub fn is_ordered(&self) -> bool {
        matches!(
            self,
            Self::UnreliableSequenced | Self::ReliableOrdered | Self::ReliableSequenced
        )
    }

    pub fn is_sequenced(&self) -> bool {
        matches!(self, Self::UnreliableSequenced | Self::ReliableSequenced)
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::Unreliable),
            0x01 => Some(Self::UnreliableSequenced),
            0x02 => Some(Self::Reliable),
            0x03 => Some(Self::ReliableOrdered),
            0x04 => Some(Self::ReliableSequenced),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Unreliable => 0x00,
            Self::UnreliableSequenced => 0x01,
            Self::Reliable => 0x02,
            Self::ReliableOrdered => 0x03,
            Self::ReliableSequenced => 0x04,
        }
    }
}

pub type BodyBytes = Vec<u8>;

#[derive(Clone, Debug)]
pub struct Frame {
    pub reliability: Reliability,

    /// Only if reliable
    pub frame_idx: u32,
    /// Only if sequenced
    pub seq: u32,
    /// Only if ordered
    pub order_idx: u32,
    pub fragment: Option<FrameFragment>,

    pub body: BodyBytes,
}

#[derive(Clone, Debug)]
pub struct FrameFragment {
    pub count: u32,
    pub index: u32,
    pub id: u16,
}

impl Message for Frame {
    fn serialize(&self, buf: &mut WriteBuf) -> Result<(), MessageError> {
        let mut header = self.reliability.to_u8() << 5;
        if self.fragment.is_some() {
            header |= FLAG_FRAGMENTED;
        }
        buf.write_u8(header)?;
        buf.write_u16((self.body.len() << 3) as u16)?;
        if self.reliability.is_reliable() {
            buf.write_u24(self.frame_idx)?;
        }
        if self.reliability.is_sequenced() {
            buf.write_u24(self.seq)?;
        }
        if self.reliability.is_ordered() {
            buf.write_u24(self.order_idx)?;
            buf.write_u8(0)?; // order channel
        }
        if let Some(fragment) = self.fragment.as_ref() {
            buf.write_u32(fragment.count)?;
            buf.write_u16(fragment.id)?;
            buf.write_u32(fragment.index)?;
        }
        buf.0.extend_from_slice(&self.body);
        Ok(())
    }

    fn deserialize(buf: &mut ReadBuf) -> Result<Self, MessageError> {
        let header = buf.read_u8()?;
        let fragmented = (header & FLAG_FRAGMENTED) != 0;
        let reliability_id = (header & 224) >> 5;
        let reliability: Reliability = Reliability::from_u8(reliability_id)
            .ok_or(MessageError::UnknownRealibility(reliability_id))?;
        let body_len = (buf.read_u16()? as usize) >> 3;
        if body_len == 0 {
            return Err(MessageError::ZeroSize);
        }

        let frame_idx = if reliability.is_reliable() {
            buf.read_u24()?
        } else {
            0
        };
        let seq = if reliability.is_sequenced() {
            buf.read_u24()?
        } else {
            0
        };
        let order_idx = if reliability.is_ordered() {
            let order_idx = buf.read_u24()?;
            // skip order channel
            buf.0.advance(1);
            order_idx
        } else {
            0
        };

        let fragment = if fragmented {
            Some(FrameFragment {
                count: buf.read_u32()?,
                id: buf.read_u16()?,
                index: buf.read_u32()?,
            })
        } else {
            None
        };

        let mut body = vec![0u8; body_len];
        buf.read_bytes(&mut body)?;

        Ok(Self {
            reliability,
            frame_idx,
            seq,
            order_idx,
            fragment,
            body,
        })
    }
}
