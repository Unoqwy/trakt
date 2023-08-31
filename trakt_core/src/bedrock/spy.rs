use bytes::{Buf, Bytes};
use raknet::{
    datatypes::ReadBuf,
    frame::Frame,
    message::{Message, RaknetMessage},
};

use crate::Direction;

use super::RaknetClient;

/// Result of spying into a datagram packet.
pub enum SpyDatagramResult {
    /// Nothing that we need to know about, ignore.
    Ignore,
    /// The datagram contains a [`RaknetMessage::DisconnectNotification`].
    Disconnect,
}

impl RaknetClient {
    /// Spies a datagram to look for a disconnect notification.
    ///
    /// Since we are looking for something specific and don't want to incur too much overhead anyway,
    /// the frames are partially decoded, only non-fragmented frames are read given this is what a disconnect
    /// notification message will be wrapped into.
    /// We don't need to bother with frame (re-)ordering either.
    ///
    /// ## Arguments
    ///
    /// * `direction` - Data flow direction
    /// * `data` - Datagram received data
    pub(super) fn spy_datagram(
        &self,
        direction: Direction,
        data: Bytes,
    ) -> anyhow::Result<SpyDatagramResult> {
        let mut buf = ReadBuf::new(data);
        let _ = buf.read_u8()?; // header flags
        let _ = buf.read_u24()?; // seq
        while buf.0.has_remaining() {
            let frame = Frame::deserialize(&mut buf)?;
            if frame.fragment.is_some() || frame.body.is_empty() {
                continue;
            }
            if frame.body[0] == raknet::GAME_PACKET_HEADER {
                // we could spy into game packets to look for a Disconnect packet but it may not really be worth it
                // what happens currently is that when the client receives a Disconnect packet it closes the connection
                // and never sends an ACK, so the server tries to send the packet in a loop for a few seconds
                // it's pretty negligible, I don't think it matters much
                continue;
            }
            let message_type = RaknetMessage::from_u8(frame.body[0]);
            log::trace!(
                "{} Frame with message type {:?} ({:02x}) and body size {}",
                self.debug_prefix(direction),
                message_type,
                frame.body[0],
                frame.body.len(),
            );
            if matches!(message_type, Some(RaknetMessage::DisconnectNotification)) {
                return Ok(SpyDatagramResult::Disconnect);
            }
        }
        Ok(SpyDatagramResult::Ignore)
    }
}
