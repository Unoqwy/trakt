use std::{
    sync::Arc,
    time::{self, Duration, SystemTime},
};

use anyhow::Context;
use bytes::Bytes;
use tokio::{
    net::{ToSocketAddrs, UdpSocket},
    time::Instant,
};

use crate::raknet::{
    datatypes::ReadBuf,
    message::{Message, MessageUnconnectedPing, RaknetMessage},
};

use super::message::MessageUnconnectedPong;
use ppp::v2 as haproxy;

/// Structured bedrock MOTD representation.
#[derive(Clone, Debug)]
pub struct Motd {
    /// UUID of the server
    pub server_uuid: i64,
    /// Edition of the game run by the server
    pub edition: BedrockEdition,
    /// Protocol version of the server
    pub protocol_version: u16,
    /// Display name of the server version
    pub version_name: String,

    /// MOTD Custom Text (two lines)
    pub lines: [String; 2],
    /// Online player count
    pub player_count: usize,
    /// Maximum player count
    pub max_player_count: usize,
    /// Gamemode.
    pub gamemode: GameMode,
    /// Nintendo limited.
    pub nintendo_limited: bool,

    /// Server port (IPv4)
    pub port_v4: u16,
    /// Server port (IPv6)
    pub port_v6: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BedrockEdition {
    PocketEdition,
    EducationEdition,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameMode {
    Survival,
    Creative,
    Custom(String),
}

impl Motd {
    /// Encodes a MOTD into a string payload that clients understand.
    pub fn encode_payload(&self) -> String {
        let edition = match &self.edition {
            BedrockEdition::PocketEdition => "MCPE",
            BedrockEdition::EducationEdition => "MCBE",
            BedrockEdition::Custom(str) => str,
        };
        let gamemode = match &self.gamemode {
            GameMode::Survival => "Survival",
            GameMode::Creative => "Creative",
            GameMode::Custom(str) => str,
        };
        format!(
            "{edition};{};{};{};{};{};{};{};{gamemode};{};{};{};",
            self.lines[0],
            self.protocol_version,
            self.version_name,
            self.player_count,
            self.max_player_count,
            self.server_uuid,
            self.lines[1],
            (!self.nintendo_limited) as usize,
            self.port_v4,
            self.port_v6
        )
    }

    /// Decodes a MOTD payload.
    pub fn decode_payload(payload: &str) -> Option<Motd> {
        let mut parts = payload.split(';');
        let edition = match parts.next() {
            Some("MCPE") => BedrockEdition::PocketEdition,
            Some("MCBE") => BedrockEdition::EducationEdition,
            Some(custom) if !custom.is_empty() => BedrockEdition::Custom(custom.to_owned()),
            _ => return None,
        };
        let line_one = parts.next().map(str::to_owned).unwrap_or_default();
        let protocol_version = parts
            .next()
            .and_then(|ver| ver.parse::<u16>().ok())
            .unwrap_or(0);
        let version_name = parts.next().map(str::to_owned).unwrap_or_default();
        let player_count = parts
            .next()
            .and_then(|ver| ver.parse::<usize>().ok())
            .unwrap_or(0);
        let max_player_count = parts
            .next()
            .and_then(|ver| ver.parse::<usize>().ok())
            .unwrap_or(0);
        let server_uuid = parts
            .next()
            .and_then(|ver| ver.parse::<i64>().ok())
            .unwrap_or(0);
        let line_two = parts.next().map(str::to_owned).unwrap_or_default();
        let gamemode = match parts.next() {
            Some("Survival") | None => GameMode::Survival,
            Some("Creative") => GameMode::Creative,
            Some(custom) => GameMode::Custom(custom.to_owned()),
        };
        let nintendo_limited = parts
            .next()
            .and_then(|ver| ver.parse::<i8>().ok())
            .map(|i| i == 0)
            .unwrap_or(false);
        let port_v4 = parts
            .next()
            .and_then(|ver| ver.parse::<u16>().ok())
            .unwrap_or(0);
        let port_v6 = parts
            .next()
            .and_then(|ver| ver.parse::<u16>().ok())
            .unwrap_or(0);
        Some(Motd {
            server_uuid,
            edition,
            protocol_version,
            version_name,
            lines: [line_one, line_two],
            player_count,
            max_player_count,
            gamemode,
            nintendo_limited,
            port_v4,
            port_v6,
        })
    }
}

/// Pings a Bedrock server and get MOTD information.
///
/// ## Arguments
///
/// * `local_addr` - Local address to bind the UDP socket to
/// * `addr` - Address of the remote server
/// * `proxy_protocol` - Whether proxy protocol is required by the server
/// * `timeout` - Timeout duration
pub async fn ping<A1: ToSocketAddrs, A2: ToSocketAddrs>(
    local_addr: A1,
    addr: A2,
    proxy_protocol: bool,
    timeout: Duration,
) -> anyhow::Result<Motd> {
    let udp_sock = UdpSocket::bind(local_addr).await?;
    udp_sock.connect(addr).await?;

    let now = SystemTime::now()
        .duration_since(time::UNIX_EPOCH)?
        .as_secs()
        .try_into()?;
    let ping = MessageUnconnectedPing {
        client_uuid: now,
        forward_timestamp: now,
    };

    let ping_packet = if proxy_protocol {
        let local_addr = udp_sock.local_addr()?;
        let header = haproxy::Builder::with_addresses(
            haproxy::Version::Two | haproxy::Command::Proxy,
            haproxy::Protocol::Datagram,
            (local_addr, local_addr),
        )
        .build()?;

        let mut buf = header;
        buf.extend(ping.to_bytes()?);
        buf
    } else {
        ping.to_bytes()?
    };

    let udp_sock = Arc::new(udp_sock);
    let udp_sock_2 = udp_sock.clone();
    let deadline = Instant::now() + timeout;

    let mut buf = [0u8; 1492];
    let len = tokio::select! {
        res = ping_resender(udp_sock_2, &ping_packet) => {
            res?;
            0
        }
        res = tokio::time::timeout_at(deadline, udp_sock.recv(&mut buf)) => res??,
    };
    let buf = &buf[..len];
    let mut buf = ReadBuf::new(Bytes::copy_from_slice(buf));
    let message_type = RaknetMessage::from_u8(buf.read_u8()?);
    if !matches!(message_type, Some(RaknetMessage::UnconnectedPong)) {
        return Err(anyhow::anyhow!("Received a reply other than pong"));
    }
    let pong = MessageUnconnectedPong::deserialize(&mut buf)?;
    let motd = Motd::decode_payload(&pong.motd).context("empty payload")?;
    Ok(motd)
}

async fn ping_resender(udp_sock: Arc<UdpSocket>, ping_packet: &[u8]) -> anyhow::Result<()> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        log::trace!("Ping attempt #{} to {}", attempts, udp_sock.peer_addr()?);
        udp_sock.send(ping_packet).await?;
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}
