use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use thiserror::Error;

use super::bencode::{write_bytes, write_int, write_key};
use crate::core::types::{
    AnnounceEvent, AnnounceInput, AnnounceOutput, InfoHash, PeerId, TorrentStats, ID_LEN,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnounceQuery {
    pub info_hash: InfoHash,
    pub peer_id: PeerId,
    pub port: u16,
    pub left: u64,
    pub event: AnnounceEvent,
    pub numwant: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScrapeQuery {
    pub info_hashes: Vec<InfoHash>,
}

impl AnnounceQuery {
    /// Convert into the engine input. The caller resolves the peer IP
    /// (remote address, or the proxy-header IP when a trusted proxy is
    /// configured) and the client tag. The query's optional `ip`
    /// parameter is deliberately not honored: any client could spoof it
    /// to inject arbitrary addresses into peer lists.
    pub fn into_input(self, ip: IpAddr, client_tag: u8) -> AnnounceInput {
        AnnounceInput {
            info_hash: self.info_hash,
            peer_id: self.peer_id,
            ip,
            port: self.port,
            left: self.left,
            event: self.event,
            numwant: self.numwant,
            client_tag,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ProtocolError {
    #[error("missing required parameter: {0}")]
    Missing(&'static str),
    #[error("invalid parameter: {0}")]
    Invalid(&'static str),
}

pub fn parse_announce_query(raw_query: &str) -> Result<AnnounceQuery, ProtocolError> {
    let mut info_hash = None;
    let mut peer_id = None;
    let mut port = None;
    let mut left = None;
    let mut event = None;
    let mut numwant = None;

    for pair in raw_query.split('&').filter(|s| !s.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "info_hash" => info_hash = Some(parse_info_hash_value(value)?),
            "peer_id" => peer_id = Some(parse_peer_id_value(value)?),
            "port" => port = Some(value.parse().map_err(|_| ProtocolError::Invalid("port"))?),
            "left" => left = Some(value.parse().map_err(|_| ProtocolError::Invalid("left"))?),
            "event" => {
                event = Some(match value {
                    "started" => AnnounceEvent::Started,
                    "completed" => AnnounceEvent::Completed,
                    "stopped" => AnnounceEvent::Stopped,
                    "" => AnnounceEvent::Empty,
                    _ => return Err(ProtocolError::Invalid("event")),
                })
            }
            "numwant" => {
                numwant = Some(
                    value
                        .parse()
                        .map_err(|_| ProtocolError::Invalid("numwant"))?,
                )
            }
            _ => {}
        }
    }

    Ok(AnnounceQuery {
        info_hash: info_hash.ok_or(ProtocolError::Missing("info_hash"))?,
        peer_id: peer_id.ok_or(ProtocolError::Missing("peer_id"))?,
        port: port.ok_or(ProtocolError::Missing("port"))?,
        left: left.unwrap_or(0),
        event: event.unwrap_or(AnnounceEvent::Empty),
        numwant: numwant.unwrap_or(100).min(400),
    })
}

/// Maximum number of `info_hash` parameters accepted in one scrape
/// request. Without a cap, a single large URI could force an unbounded
/// amount of per-hash work (parse, shard locks, response build) on this
/// unauthenticated endpoint.
pub const MAX_SCRAPE_HASHES: usize = 200;

pub fn parse_scrape_query(raw_query: &str) -> Result<ScrapeQuery, ProtocolError> {
    let mut info_hashes = Vec::new();
    for pair in raw_query.split('&').filter(|s| !s.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key != "info_hash" {
            continue;
        }
        if info_hashes.len() == MAX_SCRAPE_HASHES {
            return Err(ProtocolError::Invalid("too many info_hash values"));
        }
        info_hashes.push(parse_info_hash_value(value)?);
    }

    if info_hashes.is_empty() {
        return Err(ProtocolError::Missing("info_hash"));
    }

    Ok(ScrapeQuery { info_hashes })
}

/// Resolve the announcing peer's IP: the trusted-proxy header IP when
/// present, otherwise the remote address.
pub fn peer_ip(header_ip: Option<IpAddr>, remote_addr: Option<SocketAddr>) -> IpAddr {
    header_ip
        .or_else(|| remote_addr.map(|addr| addr.ip()))
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

pub fn announce_response(output: AnnounceOutput) -> Vec<u8> {
    let (v4_bytes, v6_bytes) = output.peers;
    let mut buf = Vec::with_capacity(160 + v4_bytes.len() + v6_bytes.len());
    buf.push(b'd');

    write_int(&mut buf, b"complete", output.complete as i64);
    write_int(&mut buf, b"incomplete", output.incomplete as i64);
    write_int(&mut buf, b"downloaded", output.downloaded as i64);
    write_int(&mut buf, b"interval", output.interval as i64);
    write_bytes(&mut buf, b"peers", &v4_bytes);
    write_bytes(&mut buf, b"peers6", &v6_bytes);

    buf.push(b'e');
    buf
}

pub fn scrape_response(stats: HashMap<InfoHash, TorrentStats>) -> Vec<u8> {
    let mut entries: Vec<_> = stats.into_iter().collect();
    entries.sort_by_key(|(hash, _)| *hash);

    let mut buf = Vec::with_capacity(64 + entries.len() * 64);
    buf.push(b'd');
    write_key(&mut buf, b"files");
    buf.push(b'd');
    for (info_hash, stats) in entries {
        write_key(&mut buf, info_hash.as_bytes());
        buf.push(b'd');
        write_int(&mut buf, b"complete", stats.complete as i64);
        write_int(&mut buf, b"downloaded", stats.downloaded as i64);
        write_int(&mut buf, b"incomplete", stats.incomplete as i64);
        buf.push(b'e');
    }
    buf.push(b'e');
    buf.push(b'e');
    buf
}

fn parse_info_hash_value(value: &str) -> Result<InfoHash, ProtocolError> {
    Ok(InfoHash(parse_20_byte_raw(value, "info_hash")?))
}

fn parse_peer_id_value(value: &str) -> Result<PeerId, ProtocolError> {
    Ok(PeerId(parse_20_byte_raw(value, "peer_id")?))
}

fn parse_20_byte_raw(value: &str, name: &'static str) -> Result<[u8; ID_LEN], ProtocolError> {
    let bytes = value.as_bytes();
    let mut buf = [0u8; ID_LEN];
    let mut pos = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if pos >= ID_LEN {
            return Err(ProtocolError::Invalid(name));
        }
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_decode(bytes[i + 1]).ok_or(ProtocolError::Invalid(name))?;
                let lo = hex_decode(bytes[i + 2]).ok_or(ProtocolError::Invalid(name))?;
                buf[pos] = (hi << 4) | lo;
                i += 3;
            }
            b'+' => {
                buf[pos] = b' ';
                i += 1;
            }
            b => {
                buf[pos] = b;
                i += 1;
            }
        }
        pos += 1;
    }

    if pos == ID_LEN {
        Ok(buf)
    } else {
        Err(ProtocolError::Invalid(name))
    }
}

fn hex_decode(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::{announce_response, parse_announce_query, parse_scrape_query};
    use crate::core::types::AnnounceOutput;

    #[test]
    fn parses_binary_announce_fields() {
        let query = "info_hash=%00%01%02%03%04%05%06%07%08%09%0A%0B%0C%0D%0E%0F%10%11%12%13&peer_id=-RT0001-abcdefgh1234&port=6881&left=10&event=started";
        let parsed = parse_announce_query(query).unwrap();

        assert_eq!(parsed.info_hash.0[0], 0);
        assert_eq!(parsed.info_hash.0[19], 19);
        assert_eq!(parsed.peer_id.0, *b"-RT0001-abcdefgh1234");
        assert_eq!(parsed.port, 6881);
    }

    #[test]
    fn parses_multiple_scrape_hashes() {
        let parsed =
            parse_scrape_query("info_hash=aaaaaaaaaaaaaaaaaaaa&info_hash=bbbbbbbbbbbbbbbbbbbb")
                .unwrap();

        assert_eq!(parsed.info_hashes.len(), 2);
    }

    #[test]
    fn scrape_accepts_up_to_the_hash_limit() {
        let query: String = (0..super::MAX_SCRAPE_HASHES)
            .map(|index| format!("info_hash={index:020}"))
            .collect::<Vec<_>>()
            .join("&");
        let parsed = parse_scrape_query(&query).unwrap();

        assert_eq!(parsed.info_hashes.len(), super::MAX_SCRAPE_HASHES);
    }

    #[test]
    fn scrape_rejects_more_than_the_hash_limit() {
        let query: String = (0..=super::MAX_SCRAPE_HASHES)
            .map(|index| format!("info_hash={index:020}"))
            .collect::<Vec<_>>()
            .join("&");

        assert_eq!(
            parse_scrape_query(&query),
            Err(super::ProtocolError::Invalid("too many info_hash values"))
        );
    }

    #[test]
    fn announce_response_compact_ipv4() {
        let output = AnnounceOutput {
            interval: 1800,
            complete: 1,
            incomplete: 0,
            downloaded: 0,
            peers: (vec![127, 0, 0, 1, 0x1a, 0xe1], vec![]),
        };
        let encoded = announce_response(output);
        assert!(encoded.windows(6).any(|w| w == [127, 0, 0, 1, 0x1a, 0xe1]));
    }

    #[test]
    fn announce_response_compact_ipv6() {
        let mut v6 = Ipv6Addr::LOCALHOST.octets().to_vec();
        v6.extend_from_slice(&6881_u16.to_be_bytes());
        let output = AnnounceOutput {
            interval: 1800,
            complete: 1,
            incomplete: 0,
            downloaded: 0,
            peers: (vec![], v6.clone()),
        };
        let encoded = announce_response(output);
        assert!(encoded.windows(18).any(|w| w == v6.as_slice()));
    }
}
