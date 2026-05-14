use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};

use percent_encoding::percent_decode_str;
use thiserror::Error;

use crate::bencode::Value;
use crate::tracker::AnnounceOutput;
use crate::types::{AnnounceEvent, InfoHash, PeerContact, PeerId, TorrentStats, ID_LEN};

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
    let params = parse_query(raw_query);

    let info_hash = parse_info_hash(required_first(&params, "info_hash")?)?;
    let peer_id = parse_peer_id(required_first(&params, "peer_id")?)?;
    let port = parse_u16(required_first(&params, "port")?, "port")?;
    let uploaded = parse_optional_u64(&params, "uploaded")?.unwrap_or(0);
    let downloaded = parse_optional_u64(&params, "downloaded")?.unwrap_or(0);
    let left = parse_optional_u64(&params, "left")?.unwrap_or(0);
    let event = params
        .get("event")
        .and_then(|values| values.first())
        .map(|value| match value.as_str() {
            "started" => Ok(AnnounceEvent::Started),
            "completed" => Ok(AnnounceEvent::Completed),
            "stopped" => Ok(AnnounceEvent::Stopped),
            "" => Ok(AnnounceEvent::Empty),
            _ => Err(ProtocolError::Invalid("event")),
        })
        .transpose()?
        .unwrap_or(AnnounceEvent::Empty);
    let numwant = parse_optional_usize(&params, "numwant")?
        .unwrap_or(100)
        .min(400);
    let compact = parse_optional_u64(&params, "compact")?.unwrap_or(1) != 0;
    let ip = params
        .get("ip")
        .and_then(|values| values.first())
        .map(|value| value.parse().map_err(|_| ProtocolError::Invalid("ip")))
        .transpose()?;

    Ok(AnnounceQuery {
        info_hash,
        peer_id,
        ip,
        port,
        uploaded,
        downloaded,
        left,
        event,
        numwant,
        compact,
    })
}

pub fn parse_scrape_query(raw_query: &str) -> Result<ScrapeQuery, ProtocolError> {
    let params = parse_query(raw_query);
    let values = params
        .get("info_hash")
        .ok_or(ProtocolError::Missing("info_hash"))?;

    if values.is_empty() {
        return Err(ProtocolError::Missing("info_hash"));
    }

    let info_hashes = values
        .iter()
        .map(|value| parse_info_hash(value))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ScrapeQuery { info_hashes })
}

pub fn peer_ip(query_ip: Option<IpAddr>, remote_addr: Option<SocketAddr>) -> IpAddr {
    query_ip
        .or_else(|| remote_addr.map(|addr| addr.ip()))
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

pub fn announce_response(output: AnnounceOutput, compact: bool) -> Vec<u8> {
    let (peers, peers6) = if compact {
        (
            Value::bytes(compact_peers(&output.peers)),
            Value::bytes(compact_peers6(&output.peers)),
        )
    } else {
        (
            Value::List(output.peers.iter().map(peer_dictionary).collect()),
            Value::bytes(Vec::new()),
        )
    };

    Value::dictionary([
        (b"complete".to_vec(), Value::integer(output.complete as i64)),
        (
            b"incomplete".to_vec(),
            Value::integer(output.incomplete as i64),
        ),
        (b"interval".to_vec(), Value::integer(output.interval as i64)),
        (b"peers".to_vec(), peers),
        (b"peers6".to_vec(), peers6),
    ])
    .encode()
}

pub fn scrape_response(stats: HashMap<InfoHash, TorrentStats>) -> Vec<u8> {
    let mut files = BTreeMap::new();

    for (info_hash, stats) in stats {
        files.insert(
            info_hash.as_bytes().to_vec(),
            Value::dictionary([
                (b"complete".to_vec(), Value::integer(stats.complete as i64)),
                (
                    b"downloaded".to_vec(),
                    Value::integer(stats.downloaded as i64),
                ),
                (
                    b"incomplete".to_vec(),
                    Value::integer(stats.incomplete as i64),
                ),
            ]),
        );
    }

    Value::dictionary([(b"files".to_vec(), Value::Dictionary(files))]).encode()
}

pub fn compact_peers(peers: &[PeerContact]) -> Vec<u8> {
    let mut compact = Vec::with_capacity(peers.len() * 6);

    for peer in peers {
        if let Some(bytes) = peer.compact_ipv4() {
            compact.extend_from_slice(&bytes);
        }
    }

    compact
}

pub fn compact_peers6(peers: &[PeerContact]) -> Vec<u8> {
    let mut compact = Vec::with_capacity(peers.len() * 18);

    for peer in peers {
        if let Some(bytes) = peer.compact_ipv6() {
            compact.extend_from_slice(&bytes);
        }
    }

    compact
}

fn peer_dictionary(peer: &PeerContact) -> Value {
    Value::dictionary([
        (b"ip".to_vec(), Value::string(peer.ip.to_string())),
        (b"port".to_vec(), Value::integer(peer.port as i64)),
    ])
}

fn parse_query(raw_query: &str) -> HashMap<String, Vec<String>> {
    let mut params = HashMap::<String, Vec<String>>::new();

    for pair in raw_query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_query_component_lossy(key);
        params.entry(key).or_default().push(value.replace('+', " "));
    }

    params
}

fn required_first<'a>(
    params: &'a HashMap<String, Vec<String>>,
    key: &'static str,
) -> Result<&'a str, ProtocolError> {
    params
        .get(key)
        .and_then(|values| values.first())
        .map(String::as_str)
        .ok_or(ProtocolError::Missing(key))
}

fn parse_info_hash(value: &str) -> Result<InfoHash, ProtocolError> {
    Ok(InfoHash(parse_20_byte_value(value, "info_hash")?))
}

fn parse_peer_id(value: &str) -> Result<PeerId, ProtocolError> {
    Ok(PeerId(parse_20_byte_value(value, "peer_id")?))
}

fn parse_20_byte_value(value: &str, name: &'static str) -> Result<[u8; ID_LEN], ProtocolError> {
    let bytes = percent_decode_str(value).collect::<Vec<_>>();
    bytes.try_into().map_err(|_| ProtocolError::Invalid(name))
}

fn decode_query_component_lossy(value: &str) -> String {
    percent_decode_str(value).decode_utf8_lossy().into_owned()
}

fn parse_u16(value: &str, name: &'static str) -> Result<u16, ProtocolError> {
    value.parse().map_err(|_| ProtocolError::Invalid(name))
}

fn parse_optional_u64(
    params: &HashMap<String, Vec<String>>,
    key: &'static str,
) -> Result<Option<u64>, ProtocolError> {
    params
        .get(key)
        .and_then(|values| values.first())
        .map(|value| value.parse().map_err(|_| ProtocolError::Invalid(key)))
        .transpose()
}

fn parse_optional_usize(
    params: &HashMap<String, Vec<String>>,
    key: &'static str,
) -> Result<Option<usize>, ProtocolError> {
    params
        .get(key)
        .and_then(|values| values.first())
        .map(|value| value.parse().map_err(|_| ProtocolError::Invalid(key)))
        .transpose()
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::{compact_peers, compact_peers6, parse_announce_query, parse_scrape_query};
    use crate::types::PeerContact;

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
    fn encodes_compact_ipv4_peers() {
        let peers = [PeerContact {
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 6881,
        }];

        assert_eq!(compact_peers(&peers), vec![127, 0, 0, 1, 0x1a, 0xe1]);
    }

    #[test]
    fn encodes_compact_ipv6_peers() {
        let peers = [PeerContact {
            ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
            port: 6881,
        }];

        let mut expected = Ipv6Addr::LOCALHOST.octets().to_vec();
        expected.extend_from_slice(&6881_u16.to_be_bytes());

        assert_eq!(compact_peers6(&peers), expected);
    }
}
