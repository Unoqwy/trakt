use rand::Rng;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use crate::config::ConfigProvider;
use crate::health::HealthController;
use crate::load_balancer::{BackendServer, LoadBalancer};
use crate::motd::MOTDReflector;
use crate::raknet;
use crate::raknet::{
    datatypes::ReadBuf,
    frame::Frame,
    message::{Message, MessageUnconnectedPing, MessageUnconnectedPong, RaknetMessage},
};
use crate::scheduler::Scheduler;
use bytes::{Buf, Bytes};
use tokio::{
    net::{ToSocketAddrs, UdpSocket},
    sync::{Notify, RwLock, Semaphore},
};

use ppp::v2 as haproxy;

/// Raknet proxy server that manage connections and use
/// the load balancers to the server for new connections.
///
/// It will forward all the traffic, except offline (no initialized Raknet connection
/// with the server) MOTD requests.
pub struct RaknetProxy {
    /// UDP socket for Player <-> Proxy traffic.
    in_udp_sock: Arc<UdpSocket>,
    /// Cached port from `in_udp_sock`.
    in_bound_port: u16,

    /// Random ID consistent during the lifetime of the proxy
    /// representing the server.
    server_uuid: i64,
    /// All current clients of the proxy.
    clients: Arc<RwLock<HashMap<SocketAddr, Arc<RaknetClient>>>>,

    /// Config provider.
    config_provider: Arc<ConfigProvider>,
    /// MOTD reflector.
    motd_reflector: Arc<MOTDReflector>,
    /// Load balancer.
    load_balancer: LoadBalancer,
    /// Scheduler.
    scheduler: Scheduler,
}

/// A client to the proxy.
///
/// Since UDP is a connectionless protocol, any mention of "connection"
/// is in fact an emulated connection, aka. session.
struct RaknetClient {
    /// Remote player client address.
    addr: SocketAddr,
    /// Backend server.
    server: Arc<BackendServer>,
    /// UDP socket for Player <-> Proxy traffic.
    proxy_udp_sock: Arc<UdpSocket>,
    /// UDP socket for Proxy <-> Server traffic.
    udp_sock: UdpSocket,
    /// Cached local socket address of `udp_sock`.
    udp_sock_addr: SocketAddr,
    /// Connection stage.
    stage: RwLock<ConnectionStage>,

    /// Close notifier.
    close_notify: Notify,
    /// Semaphore used to wait for guaranteed close state.
    close_lock: Semaphore,
}

/// The stage at which a connection is at.
enum ConnectionStage {
    /// Processing Raknet handshake packets (open connection 1 & 2).
    Handshake,
    /// Past Raknet handshake packets. May still be in Game handshake.
    Connected,
    /// The connection is closed.
    Closed,
}

/// Result of spying into a datagram packet.
enum SpyDatagramResult {
    /// Nothing that we need to know about, ignore.
    Ignore,
    /// The datagram contains a [`RaknetMessage::DisconnectNotification`].
    Disconnect,
}

/// Data flow direction.
#[derive(Debug, Clone, Copy)]
enum Direction {
    /// Player <-> Server
    PlayerToServer,
    /// Server <-> Player
    ServerToPlayer,
}

impl RaknetProxy {
    /// Attempts to bind a proxy server to a UDP socket.
    ///
    /// ## Arguments
    ///
    /// * `in_addr` - Address to bind to for Player <-> Proxy traffic
    /// * `config_provider` - Config provider
    pub async fn bind<A: ToSocketAddrs>(
        in_addr: A,
        config_provider: Arc<ConfigProvider>,
    ) -> std::io::Result<Arc<Self>> {
        let in_udp_sock = UdpSocket::bind(in_addr).await?;
        let in_bound_port = in_udp_sock.local_addr()?.port();
        let server_uuid = rand::thread_rng().gen();
        let motd_reflector = Arc::new(MOTDReflector::new(config_provider.clone()));
        let health_controller = Arc::new(HealthController::new(config_provider.clone()));
        let load_balancer =
            LoadBalancer::init(config_provider.clone(), health_controller.clone()).await;
        let scheduler = Scheduler::new(
            config_provider.clone(),
            motd_reflector.clone(),
            health_controller,
        );
        Ok(Arc::new(Self {
            in_udp_sock: Arc::new(in_udp_sock),
            in_bound_port,
            server_uuid,
            config_provider,
            clients: Default::default(),
            motd_reflector,
            load_balancer,
            scheduler,
        }))
    }

    /// Reloads configuration.
    pub async fn reload_config(&self) {
        self.load_balancer.reload_config().await;
        self.scheduler.restart().await;
    }

    /// Runs the proxy server.
    ///
    /// If stopped graciously it will return `Ok(())`, otherwise it will return an error.
    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        self.scheduler.start();
        log::debug!(
            "Starting Raknet proxy server on {}",
            self.in_udp_sock.local_addr()?
        );

        let udp_sock = self.in_udp_sock.clone();
        let mut buf = [0u8; 1492];
        loop {
            let (len, addr) = udp_sock.recv_from(&mut buf).await?;
            let data = Bytes::copy_from_slice(&buf[..len]);

            tokio::spawn({
                let __self = self.clone();
                async move {
                    if let Err(err) = __self.handle_recv(addr, data).await {
                        log::debug!(
                            "[{}] Unable to handle player -> server UDP datagram message: {:?}",
                            addr,
                            err
                        );
                    }
                }
            });
        }
    }

    /// Performs a cleanup after the proxy stopped.
    pub async fn cleanup(&self) {
        self.scheduler.stop(true).await;
    }

    /// Handles incoming data from the UDP socket from the player to the server.
    ///
    /// ## Arguments
    ///
    /// * `addr` - Remote player client address
    /// * `data` - Raw received data
    async fn handle_recv(&self, addr: SocketAddr, data: Bytes) -> anyhow::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let message_type = RaknetMessage::from_u8(data[0]);
        let client = {
            let clients = self.clients.read().await;
            clients.get(&addr).cloned()
        };
        match (message_type, client) {
            (
                Some(
                    RaknetMessage::UnconnectedPing | RaknetMessage::UnconnectedPingOpenConnections,
                ),
                _,
            ) => {
                let mut buf = ReadBuf::new(data);
                let _ = buf.read_u8()?;
                self.handle_unconnected_ping(addr, buf).await?;
            }
            (_, Some(client))
                if matches!(*client.stage.read().await, ConnectionStage::Connected) =>
            {
                if let Err(err) = client.handle_incoming_player(data).await {
                    log::debug!(
                        "{} Unable to handle UDP datagram message: {:?}",
                        client.debug_prefix(Direction::PlayerToServer),
                        err
                    );
                }
            }
            (Some(message_type), mut client) => {
                log::trace!("[{}] Received offline message {:?}", addr, message_type);
                if client.is_none() || message_type.eq(&RaknetMessage::OpenConnectionRequest1) {
                    if let Some(client) = client {
                        client.close_notify.notify_one();
                        let _ = client.close_lock.acquire().await;
                    }
                    let new_client = self.new_client(addr, ConnectionStage::Handshake).await?;
                    client = Some(new_client);
                }
                client.unwrap().forward_to_server(&data).await;
            }
            _ => {}
        }
        Ok(())
    }

    /// Creates and insert a new client.
    /// The caller is responsible for ensuring it would not overwrite an existing client,
    /// otherwise an error will be returned and the client won't be created.
    ///
    /// ## Arguments
    ///
    /// * `addr` - Remote player client address
    /// * `stage` - Connection stage. Should always be [`ConnectionStage::Handshake`].
    async fn new_client(
        &self,
        addr: SocketAddr,
        stage: ConnectionStage,
    ) -> anyhow::Result<Arc<RaknetClient>> {
        let (proxy_bind, proxy_protocol) = {
            let config = self.config_provider.read().await;
            (
                config.proxy_bind.clone(),
                config.proxy_protocol.unwrap_or(true),
            )
        };
        let sock = UdpSocket::bind(proxy_bind).await?;
        let mut clients = self.clients.write().await;
        if clients.contains_key(&addr) {
            return Err(anyhow::anyhow!(
                "Failed to maintain state for client {}",
                addr
            ));
        }
        let server = match self.load_balancer.next().await {
            Some(server) => server,
            None => return Err(anyhow::anyhow!("No server available to proxy this player")),
        };
        log::debug!("[{}] Picked server {}", addr, server.addr);
        let client = Arc::new(RaknetClient {
            addr,
            server,
            proxy_udp_sock: self.in_udp_sock.clone(),
            udp_sock_addr: sock.local_addr()?,
            udp_sock: sock,
            stage: RwLock::new(stage),
            close_notify: Notify::new(),
            close_lock: Semaphore::new(0),
        });
        clients.insert(addr, client.clone());
        tokio::spawn({
            let client = client.clone();
            let clients = self.clients.clone();
            async move {
                client.server.load.fetch_add(1, Ordering::Relaxed);
                let loop_result = client.run_event_loop().await;
                let client_count = {
                    let mut clients = clients.write().await;
                    clients.remove(&client.addr);
                    clients.len()
                };
                let was_connected = {
                    let mut w = client.stage.write().await;
                    let was_connected = matches!(*w, ConnectionStage::Connected);
                    *w = ConnectionStage::Closed;
                    was_connected
                };
                client.close_lock.add_permits(1);
                client.server.load.fetch_sub(1, Ordering::Relaxed);
                match loop_result {
                    Ok(_) => {
                        log::debug!(
                            "Connection closed: {} | {} total",
                            client.addr,
                            client_count,
                        );
                    }
                    Err(err) => {
                        log::debug!(
                            "Connection closed unexpectedly for {}: {} | {} total",
                            client.addr,
                            err,
                            client_count
                        );
                    }
                }
                if was_connected {
                    log::info!(
                        "Player {} has disconnected from {}",
                        client.addr,
                        client.server.addr
                    )
                }
            }
        });
        log::debug!(
            "Client initialized: {} <-> {} ({}) | {} total",
            client.addr,
            client.server.addr,
            client.udp_sock.local_addr()?,
            clients.len()
        );
        if proxy_protocol {
            client.send_haproxy_info().await?;
        }
        Ok(client)
    }

    /// Handles a ping request from an offline message (aka. unconnected ping request).
    ///
    /// ## Arguments
    ///
    /// * `addr` - Remote player client address
    /// * `buf` - Buffer to read the request from
    async fn handle_unconnected_ping(
        &self,
        addr: SocketAddr,
        mut buf: ReadBuf,
    ) -> anyhow::Result<()> {
        let ping = MessageUnconnectedPing::deserialize(&mut buf)?;

        let server_uuid = self.server_uuid;
        let motd_payload = match self.motd_reflector.last_motd().await {
            Some(mut motd) => {
                motd.server_uuid = server_uuid;
                motd.port_v4 = self.in_bound_port;
                motd.port_v6 = motd.port_v4;
                if motd.lines[0].is_empty() {
                    // motd reply has no effect with an empty title
                    motd.lines[0] = "...".into();
                }
                motd.encode_payload()
            }
            None => String::new(),
        };

        let pong = MessageUnconnectedPong {
            timestamp: ping.forward_timestamp,
            server_uuid,
            motd: motd_payload,
        };
        self.in_udp_sock.send_to(&pong.to_bytes()?, addr).await?;
        Ok(())
    }
}

impl RaknetClient {
    /// Sends a packet with HAProxy protocol header.
    async fn send_haproxy_info(&self) -> anyhow::Result<()> {
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
    async fn run_event_loop(&self) -> anyhow::Result<()> {
        let mut buf = [0u8; 1492];
        // 10 seconds without data from the server = force close
        let timeout = Duration::from_secs(10);
        loop {
            tokio::select! {
                _ = self.close_notify.notified() => return Ok(()),

                result = tokio::time::timeout(timeout, self.udp_sock.recv(&mut buf)) => {
                    let len = result??;
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
                log::info!("Player {} has connected to {}", self.addr, self.server.addr)
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
            self.close_notify.notify_one();
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
    async fn handle_incoming_player(&self, data: Bytes) -> anyhow::Result<()> {
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
            self.close_notify.notify_one();
        }
        Ok(())
    }

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
    fn spy_datagram(&self, direction: Direction, data: Bytes) -> anyhow::Result<SpyDatagramResult> {
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

    /// Forwards data received from the player to the server.
    ///
    /// ## Arguments
    ///
    /// * `data` - Raw data received from the player
    #[inline]
    async fn forward_to_server(&self, data: &[u8]) {
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
    fn debug_prefix(&self, direction: Direction) -> String {
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
