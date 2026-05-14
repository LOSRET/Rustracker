use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const ID_LEN: usize = 20;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct InfoHash(pub [u8; ID_LEN]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct PeerId(pub [u8; ID_LEN]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnnounceEvent {
    Started,
    Completed,
    Stopped,
    Empty,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ipv4PeerKey {
    pub ip: [u8; 4],
    pub port: u16,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ipv6PeerKey {
    pub ip: [u8; 16],
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct PeerState {
    pub complete: bool,
    pub last_seen_secs: u32,
    pub client_tag: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TorrentStats {
    pub complete: usize,
    pub downloaded: u64,
    pub incomplete: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerContact {
    pub ip: IpAddr,
    pub port: u16,
}

impl InfoHash {
    pub fn new(bytes: [u8; ID_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; ID_LEN] {
        &self.0
    }

    /// Parse a 40-character hex string into an InfoHash.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != ID_LEN * 2 {
            return None;
        }
        let mut bytes = [0u8; ID_LEN];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            bytes[i] = hex_byte(chunk)?;
        }
        Some(Self(bytes))
    }
}

fn hex_byte(pair: &[u8]) -> Option<u8> {
    if pair.len() != 2 {
        return None;
    }
    let hi = hex_val(pair[0])?;
    let lo = hex_val(pair[1])?;
    Some((hi << 4) | lo)
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl PeerId {
    pub fn new(bytes: [u8; ID_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; ID_LEN] {
        &self.0
    }
}

impl PeerState {
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

impl Ipv4PeerKey {
    pub fn new(ip: Ipv4Addr, port: u16) -> Self {
        Self {
            ip: ip.octets(),
            port,
        }
    }

    pub fn contact(self) -> PeerContact {
        PeerContact {
            ip: IpAddr::V4(Ipv4Addr::from(self.ip)),
            port: self.port,
        }
    }

    pub fn compact(self) -> [u8; 6] {
        let mut out = [0_u8; 6];
        out[..4].copy_from_slice(&self.ip);
        out[4..].copy_from_slice(&self.port.to_be_bytes());
        out
    }
}

impl Ipv6PeerKey {
    pub fn new(ip: Ipv6Addr, port: u16) -> Self {
        Self {
            ip: ip.octets(),
            port,
        }
    }

    pub fn contact(self) -> PeerContact {
        PeerContact {
            ip: IpAddr::V6(Ipv6Addr::from(self.ip)),
            port: self.port,
        }
    }

    pub fn compact(self) -> [u8; 18] {
        let mut out = [0_u8; 18];
        out[..16].copy_from_slice(&self.ip);
        out[16..].copy_from_slice(&self.port.to_be_bytes());
        out
    }
}

impl PeerContact {
    pub fn compact_ipv4(&self) -> Option<[u8; 6]> {
        let IpAddr::V4(ip) = self.ip else {
            return None;
        };

        let mut out = [0_u8; 6];
        out[..4].copy_from_slice(&ip.octets());
        out[4..].copy_from_slice(&self.port.to_be_bytes());
        Some(out)
    }

    pub fn compact_ipv6(&self) -> Option<[u8; 18]> {
        let IpAddr::V6(ip) = self.ip else {
            return None;
        };

        let mut out = [0_u8; 18];
        out[..16].copy_from_slice(&ip.octets());
        out[16..].copy_from_slice(&self.port.to_be_bytes());
        Some(out)
    }

    pub fn localhost(_peer_id: PeerId, port: u16) -> Self {
        Self {
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
        }
    }
}

impl fmt::Display for InfoHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
