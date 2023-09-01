use std::{
    collections::{hash_map::Entry, HashMap},
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::SystemTime,
};

use anyhow::Context;
use bytes::Bytes;
use raknet::{
    datatypes::ReadBuf,
    message::{Message, MessageUnconnectedPing, MessageUnconnectedPong, RaknetMessage},
};
use tokio::{
    net::{ToSocketAddrs, UdpSocket},
    sync::{mpsc, RwLock, Semaphore},
};

use crate::{
    config::RuntimeConfigProvider, snapshot::RecoverableProxyServer, Backend, BackendPlatform,
    BackendServer, Direction, DisconnectCause, ProxyServer,
};

use super::{
    snapshot::{RaknetClientSnapshot, RaknetProxySnapshot},
    ConnectionStage, RaknetClient,
};

/// Raknet proxy server that manage connections and use
/// the load balancers to the server for new connections.
///
/// It will forward all the traffic, except offline (no initialized Raknet connection
/// with the server) MOTD requests.
pub struct RaknetProxyServer {
    /// UDP socket for Player <-> Proxy traffic.
    in_udp_sock: Arc<UdpSocket>,
    /// Cached port from `in_udp_sock`.
    in_bound_port: u16,

    /// Active clients known by the proxy.
    clients: Arc<RwLock<HashMap<SocketAddr, Arc<RaknetClient>>>>,
    /// Backend. Domain matching for reverse proxying is not possible
    /// for Bedrock Edition (afaik), so there can only be one backend
    /// per proxy bind IP address.
    backend: RwLock<Option<Arc<Backend>>>,

    // Runtime config provider.
    config_provider: Arc<RuntimeConfigProvider>,
}

impl RaknetProxyServer {
    /// Attempts to bind the proxy server to a UDP socket.
    ///
    /// ## Arguments
    ///
    /// * `in_addr` - Address to bind to for Player <-> Proxy traffic
    /// * `config_provider` - Runtime config provider
    /// * `backend` - Initial backend
    pub async fn bind<A: ToSocketAddrs>(
        in_addr: A,
        config_provider: Arc<RuntimeConfigProvider>,
        backend: Option<Arc<Backend>>,
    ) -> std::io::Result<Self> {
        let in_udp_sock = UdpSocket::bind(in_addr).await?;
        let in_bound_port = in_udp_sock.local_addr()?.port();
        Ok(Self {
            in_udp_sock: Arc::new(in_udp_sock),
            in_bound_port,
            clients: Default::default(),
            backend: RwLock::new(backend),
            config_provider,
        })
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
                        let _ = client.close_tx.send(DisconnectCause::Unknown).await;
                        let _ = client.close_lock.acquire().await;
                    }
                    let backend = self.backend.read().await;
                    let backend = backend.as_ref().context("no backend")?;
                    let server = match backend.load_balancer.next().await {
                        Some(server) => {
                            log::debug!("[{}] Picked server {}", addr, server.addr);
                            server
                        }
                        None => {
                            return Err(anyhow::anyhow!("No server available to proxy this player"))
                        }
                    };
                    let new_client = self
                        .new_client(addr, ConnectionStage::Handshake, None, server)
                        .await?;
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
    /// * `stage` - Connection stage. Should be [`ConnectionStage::Handshake`] for new ones
    /// * `proxy_bind` - Specific Proxy <-> Server bind socket address. If [`None`], the
    ///                  default one will be used
    /// * `server` - Backend server.
    async fn new_client(
        &self,
        addr: SocketAddr,
        stage: ConnectionStage,
        proxy_bind: Option<String>,
        server: Arc<BackendServer>,
    ) -> anyhow::Result<Arc<RaknetClient>> {
        let proxy_bind = match proxy_bind {
            Some(addr) => addr,
            None => {
                let config = self.config_provider.read().await;
                config.proxy_bind.clone()
            }
        };
        let sock = UdpSocket::bind(proxy_bind).await?;
        let mut clients = self.clients.write().await;
        if clients.contains_key(&addr) {
            return Err(anyhow::anyhow!(
                "Failed to maintain state for client {}",
                addr
            ));
        }
        let (tx, rx) = mpsc::channel(1);
        let client = Arc::new(RaknetClient {
            addr,
            server: server.clone(),
            proxy_udp_sock: self.in_udp_sock.clone(),
            udp_sock_addr: sock.local_addr()?,
            udp_sock: sock,
            stage: RwLock::new(stage),
            close_tx: tx,
            close_lock: Semaphore::new(0),
        });
        clients.insert(addr, client.clone());
        tokio::spawn({
            let client = client.clone();
            let clients = self.clients.clone();
            async move {
                server.modify_load(1).await;
                let loop_result = client.run_event_loop(rx).await;
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
                {
                    let mut state = server.state.write().await;
                    state.load_score = state.load_score.saturating_sub(1);
                    state.connected_players.remove(&client.addr);
                }
                let cause = match loop_result {
                    Ok(cause) => {
                        log::debug!(
                            "Connection closed: {} | {} total",
                            client.addr,
                            client_count,
                        );
                        cause
                    }
                    Err(err) => {
                        log::debug!(
                            "Connection closed unexpectedly for {}: {} | {} total",
                            client.addr,
                            err,
                            client_count
                        );
                        DisconnectCause::Error
                    }
                };
                if was_connected {
                    log::info!(
                        "Player {} has disconnected from {} ({})",
                        client.addr,
                        client.server.addr,
                        cause.to_str(),
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
        if client.server.use_proxy_protocol().await {
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

        let (last_motd, server_uuid) = {
            let backend = self.backend.read().await;
            match &backend.as_ref().context("no backend")?.platform {
                BackendPlatform::Bedrock {
                    motd_cache,
                    server_uuid,
                } => (motd_cache.last_motd().await, *server_uuid),
            }
        };
        let motd_payload = match last_motd {
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

#[async_trait::async_trait]
impl ProxyServer for RaknetProxyServer {
    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
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

    async fn get_backends(&self) -> Vec<Arc<Backend>> {
        let backend = self.backend.read().await;
        if let Some(backend) = backend.clone() {
            vec![backend.clone()]
        } else {
            Vec::new()
        }
    }
}

#[async_trait::async_trait]
impl RecoverableProxyServer for RaknetProxyServer {
    type Snapshot = RaknetProxySnapshot;

    async fn take_snapshot(&self) -> anyhow::Result<Self::Snapshot> {
        let config = {
            let config = self.config_provider.read().await;
            config.clone()
        };
        let player_proxy_bind = self.in_udp_sock.local_addr()?.to_string();
        let active_clients = self.clients.read().await;
        let mut clients = Vec::new();
        for (_, client) in active_clients.iter() {
            let stage = client.stage.read().await;
            if !matches!(*stage, ConnectionStage::Connected) {
                continue;
            }
            clients.push(RaknetClientSnapshot {
                addr: client.addr.to_string(),
                server_addr: client.server.addr.to_string(),
                server_proxy_protocol: client.server.use_proxy_protocol().await,
                proxy_server_bind: client.udp_sock.local_addr()?.to_string(),
            });
        }
        let taken_at = SystemTime::now();
        Ok(RaknetProxySnapshot {
            taken_at,
            config,
            player_proxy_bind,
            clients,
        })
    }

    async fn recover_from_snapshot(&self, snapshot: Self::Snapshot) {
        let backend = {
            let guard = self.backend.read().await;
            match guard.clone() {
                Some(backend) => backend,
                None => return,
            }
        };
        let mut backend_state = backend.state.write().await;

        let mut servers: HashMap<SocketAddr, Arc<BackendServer>> = HashMap::new();
        for client in snapshot.clients {
            let addr = match SocketAddr::from_str(&client.addr) {
                Ok(addr) => addr,
                Err(err) => {
                    log::warn!(
                        "Could not recover client {} from snapshot: Invalid address: {:?}",
                        client.addr,
                        err
                    );
                    continue;
                }
            };
            let server_addr = match SocketAddr::from_str(&client.server_addr) {
                Ok(addr) => addr,
                Err(err) => {
                    log::warn!(
                        "Could not recover client {} from snapshot: Invalid server address: {:?}",
                        client.addr,
                        err
                    );
                    continue;
                }
            };
            let server = match servers.entry(server_addr) {
                Entry::Occupied(entry) => entry.get().clone(),
                Entry::Vacant(entry) => {
                    let server = match backend_state.get_server(server_addr) {
                        Some(server) => {
                            log::debug!("Recovering server {} on active instance", server_addr);
                            server
                        }
                        None => {
                            log::debug!("Recovering server {} on stale instance", server_addr);
                            let server = Arc::new(BackendServer::new(
                                server_addr,
                                client.server_proxy_protocol,
                            ));
                            backend_state.register_server(server.clone(), true);
                            server
                        }
                    };
                    entry.insert(server).clone()
                }
            };
            if let Err(err) = self
                .new_client(
                    addr,
                    ConnectionStage::Connected,
                    Some(client.proxy_server_bind),
                    server,
                )
                .await
            {
                log::warn!(
                    "Could not recover client {} from snapshot: {:?}",
                    client.addr,
                    err
                );
            } else {
                log::info!(
                    "Recover player {}. Connected to {}",
                    client.addr,
                    server_addr
                )
            }
        }
    }
}
