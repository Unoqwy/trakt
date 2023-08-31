use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::MAGIC;

macro_rules! read_guard {
    ($self:ident, $len:expr) => {
        if $self.0.remaining() < $len {
            return Err(BufError::NotEnoughData);
        }
    };
}

/// Alias type for a u24 to make things clearer. Not an actual u24!
#[allow(non_camel_case_types)]
pub type u24 = u32;

#[derive(Clone, Debug)]
pub enum BufError {
    /// There is no more data to read
    NotEnoughData,
    /// Expected [`crate::MAGIC`] but didn't get it
    InvalidMagic,
    /// Invalid string enconding
    InvalidString,
    /// Invalid socket address
    InvalidAdrress,
}

impl From<BufError> for anyhow::Error {
    fn from(value: BufError) -> Self {
        Self::msg(format!("{:?}", value))
    }
}

#[derive(Clone, Debug)]
pub struct ReadBuf(pub Bytes);

#[derive(Clone, Debug)]
pub struct WriteBuf(pub BytesMut);

impl ReadBuf {
    pub fn new(bytes: Bytes) -> Self {
        Self(bytes)
    }
}

impl WriteBuf {
    pub fn new() -> Self {
        Self(BytesMut::new())
    }
}

impl Default for WriteBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<u8>> for ReadBuf {
    fn from(val: Vec<u8>) -> Self {
        ReadBuf(Bytes::from(val))
    }
}

impl From<&[u8]> for ReadBuf {
    fn from(val: &[u8]) -> Self {
        ReadBuf(Bytes::copy_from_slice(val))
    }
}

#[allow(dead_code)]
impl ReadBuf {
    pub fn read_u8(&mut self) -> Result<u8, BufError> {
        read_guard!(self, 1);
        Ok(self.0.get_u8())
    }

    pub fn read_bool(&mut self) -> Result<bool, BufError> {
        read_guard!(self, 1);
        Ok(self.0.get_u8() == 1)
    }

    pub fn read_magic(&mut self) -> Result<(), BufError> {
        read_guard!(self, 16);
        let mut dest = [0u8; 16];
        self.0.copy_to_slice(&mut dest);
        if dest == MAGIC {
            Ok(())
        } else {
            Err(BufError::InvalidMagic)
        }
    }

    pub fn read_i16(&mut self) -> Result<i16, BufError> {
        read_guard!(self, 2);
        Ok(self.0.get_i16())
    }

    pub fn read_u16(&mut self) -> Result<u16, BufError> {
        read_guard!(self, 2);
        Ok(self.0.get_u16())
    }

    pub fn read_u24(&mut self) -> Result<u24, BufError> {
        read_guard!(self, 3);
        let mut bytes = [0u8; 4];
        self.0.copy_to_slice(&mut bytes[..3]);
        Ok(u32::from_le_bytes(bytes))
    }

    pub fn read_u32(&mut self) -> Result<u32, BufError> {
        read_guard!(self, 4);
        Ok(self.0.get_u32())
    }

    pub fn read_i64(&mut self) -> Result<i64, BufError> {
        read_guard!(self, 8);
        Ok(self.0.get_i64())
    }

    pub fn read_str(&mut self) -> Result<String, BufError> {
        read_guard!(self, 2);
        let len = self.0.get_u16() as usize;
        read_guard!(self, len);
        let mut bytes = vec![0u8; len];
        self.0.copy_to_slice(&mut bytes);
        String::from_utf8(bytes).map_err(|_| BufError::InvalidString)
    }

    pub fn read_address(&mut self) -> Result<SocketAddr, BufError> {
        read_guard!(self, 1);
        let ip_variant = self.read_u8()?;
        if ip_variant == 4 {
            read_guard!(self, 6);

            let mut bytes = [0u8; 4];
            self.0.copy_to_slice(&mut bytes);
            let ipv4_addr = Ipv4Addr::new(!bytes[0], !bytes[1], !bytes[2], !bytes[3]);

            let port = self.0.get_u16();
            Ok(SocketAddr::new(IpAddr::V4(ipv4_addr), port))
        } else if ip_variant == 6 {
            read_guard!(self, 28);

            self.0.advance(2);
            let port = self.0.get_u16();

            self.0.advance(4);
            let mut bytes = [0u8; 16];
            self.0.copy_to_slice(&mut bytes);
            self.0.advance(4);

            Ok(SocketAddr::new(IpAddr::V6(bytes.into()), port))
        } else {
            Err(BufError::InvalidAdrress)
        }
    }

    pub fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), BufError> {
        read_guard!(self, buf.len());
        self.0.copy_to_slice(buf);
        Ok(())
    }
}

#[allow(dead_code)]
impl WriteBuf {
    pub fn write_u8(&mut self, value: u8) -> Result<(), BufError> {
        self.0.put_u8(value);
        Ok(())
    }

    pub fn write_bool(&mut self, value: bool) -> Result<(), BufError> {
        self.0.put_u8(value as u8);
        Ok(())
    }

    pub fn write_magic(&mut self) -> Result<(), BufError> {
        self.0.extend_from_slice(&MAGIC);
        Ok(())
    }

    pub fn write_i16(&mut self, value: i16) -> Result<(), BufError> {
        self.0.put_i16(value);
        Ok(())
    }

    pub fn write_u16(&mut self, value: u16) -> Result<(), BufError> {
        self.0.put_u16(value);
        Ok(())
    }

    pub fn write_u24(&mut self, value: u24) -> Result<(), BufError> {
        let bytes = value.to_le_bytes();
        self.0.extend_from_slice(&bytes[..3]);
        Ok(())
    }

    pub fn write_u32(&mut self, value: u32) -> Result<(), BufError> {
        self.0.put_u32(value);
        Ok(())
    }

    pub fn write_i64(&mut self, value: i64) -> Result<(), BufError> {
        self.0.put_i64(value);
        Ok(())
    }

    pub fn write_str(&mut self, value: &str) -> Result<(), BufError> {
        // doesn't need special encoding, seems to be limited to ascii anyway
        let bytes = value.as_bytes();
        self.0.put_u16(bytes.len() as u16);
        self.0.extend_from_slice(bytes);
        Ok(())
    }

    pub fn write_address(&mut self, value: SocketAddr) -> Result<(), BufError> {
        if let SocketAddr::V4(ipv4_addr) = value {
            self.0.put_u8(4);

            let bytes = ipv4_addr.ip().octets().map(|b| !b);
            self.0.extend_from_slice(&bytes);
            self.0.put_u16(ipv4_addr.port());
            Ok(())
        } else if let SocketAddr::V6(ipv6_addr) = value {
            self.0.put_u8(6);

            self.0.put_u16(0);
            self.0.put_u16(ipv6_addr.port());

            self.0.put_u32(0);
            let bytes = ipv6_addr.ip().octets();
            self.0.extend_from_slice(&bytes);
            self.0.put_u32(0);

            Ok(())
        } else {
            Err(BufError::InvalidAdrress)
        }
    }
}
