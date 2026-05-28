use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use thiserror::Error;

use super::bencode::{write_bytes, write_int, write_key};
use crate::core::tracker::AnnounceOutput;
use crate::core::types::{AnnounceEvent, InfoHash, PeerId, TorrentStats, ID_LEN};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnounceQuery {
    pub info_hash: InfoHash,
    pub peer_id: PeerId,
    pub ip: Option<IpAddr>,
    pub port: u16,
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
    pub event: AnnounceEvent,
    pub numwant: usize,
    pub compact: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScrapeQuery {
    pub info_hashes: Vec<InfoHash>,
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
    let mut uploaded = None;
    let mut downloaded = None;
    let mut left = None;
    let mut event = None;
    let mut numwant = None;
    let mut compact = None;
    let mut ip = None;

    for pair in raw_query.split('&').filter(|s| !s.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "info_hash" => info_hash = Some(parse_info_hash_value(value)?),
            "peer_id" => peer_id = Some(parse_peer_id_value(value)?),
            "port" => port = Some(value.parse().map_err(|_| ProtocolError::Invalid("port"))?),
            "uploaded" => {
                uploaded = Some(
                    value
                        .parse()
                        .map_err(|_| ProtocolError::Invalid("uploaded"))?,
                )
            }
            "downloaded" => {
                downloaded = Some(
                    value
                        .parse()
                        .map_err(|_| ProtocolError::Invalid("downloaded"))?,
                )
            }
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
            "compact" => {
                compact = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| ProtocolError::Invalid("compact"))?
                        != 0,
                )
            }
            "ip" => ip = Some(value.parse().map_err(|_| ProtocolError::Invalid("ip"))?),
            _ => {}
        }
    }

    Ok(AnnounceQuery {
        info_hash: info_hash.ok_or(ProtocolError::Missing("info_hash"))?,
        peer_id: peer_id.ok_or(ProtocolError::Missing("peer_id"))?,
        port: port.ok_or(ProtocolError::Missing("port"))?,
        uploaded: uploaded.unwrap_or(0),
        downloaded: downloaded.unwrap_or(0),
        left: left.unwrap_or(0),
        event: event.unwrap_or(AnnounceEvent::Empty),
        numwant: numwant.unwrap_or(100).min(400),
        compact: compact.unwrap_or(true),
        ip,
    })
}

pub fn parse_scrape_query(raw_query: &str) -> Result<ScrapeQuery, ProtocolError> {
    let info_hashes: Vec<_> = raw_query
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            if key == "info_hash" {
                Some(parse_info_hash_value(value))
            } else {
                None
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    if info_hashes.is_empty() {
        return Err(ProtocolError::Missing("info_hash"));
    }

    Ok(ScrapeQuery { info_hashes })
}

pub fn peer_ip(query_ip: Option<IpAddr>, remote_addr: Option<SocketAddr>) -> IpAddr {
    query_ip
        .or_else(|| remote_addr.map(|addr| addr.ip()))
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

pub fn announce_response(output: AnnounceOutput, _compact: bool) -> Vec<u8> {
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
    use crate::core::tracker::AnnounceOutput;

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
    fn announce_response_compact_ipv4() {
        let output = AnnounceOutput {
            interval: 1800,
            complete: 1,
            incomplete: 0,
            downloaded: 0,
            peers: (vec![127, 0, 0, 1, 0x1a, 0xe1], vec![]),
        };
        let encoded = announce_response(output, true);
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
        let encoded = announce_response(output, true);
        assert!(encoded.windows(18).any(|w| w == v6.as_slice()));
    }
}
