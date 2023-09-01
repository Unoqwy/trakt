use std::{net::SocketAddr, sync::Arc, time::Duration};

use bytes::Bytes;
use raknet::message::RaknetMessage;
use tokio::{
    net::UdpSocket,
    sync::{mpsc, RwLock, Semaphore},
};

use ppp::v2 as haproxy;

use crate::{BackendServer, Direction, DisconnectCause};

use super::spy::SpyDatagramResult;

/// A [`super::RaknetProxyServer`] client.
///
/// Since UDP is a connectionless protocol, any mention of "connection"
/// is in fact an emulated connection, aka. session.
pub struct RaknetClient {
    /// Remote player client address.
    pub addr: SocketAddr,
    /// Backend server.
    pub server: Arc<BackendServer>,
    /// UDP socket for Player <-> Proxy traffic.
    pub(super) proxy_udp_sock: Arc<UdpSocket>,
    /// UDP socket for Proxy <-> Server traffic.
    pub(super) udp_sock: UdpSocket,
    /// Cached local socket address of `udp_sock`.
    pub(super) udp_sock_addr: SocketAddr,
    /// Connection stage.
    pub(super) stage: RwLock<ConnectionStage>,

    /// Close notifier.
    pub(super) close_tx: mpsc::Sender<DisconnectCause>,
    /// Semaphore used to wait for guaranteed close state.
    pub(super) close_lock: Semaphore,
}

/// The stage at which a Raknet connection is at.
pub enum ConnectionStage {
    /// Processing Raknet handshake packets (open connection 1 & 2).
    Handshake,
    /// Past Raknet handshake packets. May still be in Game handshake.
    Connected,
    /// The connection is closed.
    Closed,
}

impl RaknetClient {
    /// Sends a packet with HAProxy protocol header.
    pub async fn send_haproxy_info(&self) -> anyhow::Result<()> {
        let header = haproxy::Builder::with_addresses(
            haproxy::Version::Two | haproxy::Command::Proxy,
            haproxy::Protocol::Datagram,
            (self.addr, self.proxy_udp_sock.local_addr()?),
        )
        .build()?;
        self.udp_sock.send_to(&header, self.server.addr).await?;
        Ok(())
    }

    /// Runs the client event loop.
    pub async fn run_event_loop(
        &self,
        mut rx: mpsc::Receiver<DisconnectCause>,
    ) -> anyhow::Result<DisconnectCause> {
        let mut buf = [0u8; 1492];
        // 10 seconds without data from the server = force close
        let timeout = Duration::from_secs(10);
        loop {
            tokio::select! {
                cause = rx.recv() => return Ok(cause.unwrap_or(DisconnectCause::Unknown)),

                res = tokio::time::timeout(timeout, self.udp_sock.recv(&mut buf)) => {
                    let len = match res {
                        Ok(res) => res?,
                        Err(_) => return Ok(DisconnectCause::TimeoutServer),
                    };
                    let data = Bytes::copy_from_slice(&buf[..len]);
                    if let Err(err) = self.handle_incoming_server(data).await {
                        log::debug!(
                            "{} Unable to handle UDP datagram message: {:?}",
                            self.debug_prefix(Direction::ServerToPlayer),
                            err
                        );
                    }
                }
            }
        }
    }

    /// Handles incoming data from the UDP socket from the server to the player.
    ///
    /// ## Arguments
    ///
    /// * `data` - Raw received data
    async fn handle_incoming_server(&self, data: Bytes) -> anyhow::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let message_type = RaknetMessage::from_u8(data[0]);
        if matches!(message_type, Some(RaknetMessage::OpenConnectionReply2)) {
            let mut w = self.stage.write().await;
            if !matches!(*w, ConnectionStage::Connected) {
                *w = ConnectionStage::Connected;
                log::info!("Player {} has connected to {}", self.addr, self.server.addr);
                let mut server_state = self.server.state.write().await;
                server_state.connected_players.insert(self.addr);
            }
        }
        if let Some(message_type) = message_type {
            log::trace!(
                "{} Relaying message {:?}",
                self.debug_prefix(Direction::ServerToPlayer),
                message_type
            );
        }
        self.forward_to_player(&data).await;
        if matches!(
            self.spy_datagram(Direction::ServerToPlayer, data),
            Ok(SpyDatagramResult::Disconnect)
        ) {
            log::debug!(
                "{} Found disconnect notification in datagram",
                self.debug_prefix(Direction::ServerToPlayer),
            );
            self.close_tx.send(DisconnectCause::Server).await?;
        }
        Ok(())
    }

    /// Forwards data received from the server to the player.
    ///
    /// ## Arguments
    ///
    /// * `data` - Raw data received from the server
    #[inline]
    async fn forward_to_player(&self, data: &[u8]) {
        if let Err(err) = self.proxy_udp_sock.send_to(data, self.addr).await {
            log::debug!(
                "{} Unable to forward data: {:?}",
                self.debug_prefix(Direction::ServerToPlayer),
                err
            );
        }
    }

    /// Handles incoming data from the UDP socket from the player to the server.
    ///
    /// ## Arguments
    ///
    /// * `data` - Raw received data
    pub(super) async fn handle_incoming_player(&self, data: Bytes) -> anyhow::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data[0] & 0x80 == 0 {
            log::trace!(
                "{} Received non-datagram data, with header {:02x}",
                self.debug_prefix(Direction::PlayerToServer),
                data[0]
            );
            // while this is technically invalid,
            // not forwarding it would make the proxy inconsistent
            self.forward_to_server(&data).await;
            return Ok(());
        }
        self.forward_to_server(&data).await;
        if matches!(
            self.spy_datagram(Direction::PlayerToServer, data),
            Ok(SpyDatagramResult::Disconnect)
        ) {
            log::debug!(
                "{} Found disconnect notification in datagram",
                self.debug_prefix(Direction::PlayerToServer),
            );
            self.close_tx.send(DisconnectCause::Normal).await?;
        }
        Ok(())
    }

    /// Forwards data received from the player to the server.
    ///
    /// ## Arguments
    ///
    /// * `data` - Raw data received from the player
    #[inline]
    pub(super) async fn forward_to_server(&self, data: &[u8]) {
        if let Err(err) = self.udp_sock.send_to(data, self.server.addr).await {
            log::debug!(
                "{} Unable to forward data: {:?}",
                self.debug_prefix(Direction::PlayerToServer),
                err
            );
        }
    }

    /// Prefix for all debug messages related to this client.
    ///
    /// ## Arguments
    ///
    /// * `direction` - Data flow direction
    pub(super) fn debug_prefix(&self, direction: Direction) -> String {
        match direction {
            Direction::PlayerToServer => format!(
                "[player: {} -> server {} ({})]",
                self.addr, self.server.addr, self.udp_sock_addr
            ),
            Direction::ServerToPlayer => format!(
                "[server: {} ({}) -> player {}]]",
                self.server.addr, self.udp_sock_addr, self.addr
            ),
        }
    }
}
