//! BitTorrent client identification from peer_id prefix.

// ── Client ID constants ──────────────────────────────────────────────────────

pub const UNKNOWN: u8 = 0;
pub const QBITTORRENT: u8 = 1;
pub const TRANSMISSION: u8 = 2;
pub const UTORRENT: u8 = 3;
pub const BITTORRENT: u8 = 4;
pub const LIBTORRENT: u8 = 5;
pub const BITCOMET: u8 = 6;
pub const DELUGE: u8 = 7;
pub const VUZE: u8 = 8;
pub const BIGLYBT: u8 = 9;
pub const XUNLEI: u8 = 10;
pub const XUNLEI_FAST: u8 = 11;
pub const ARIA2: u8 = 12;
pub const FDM: u8 = 13;
pub const MAINLINE: u8 = 14;
pub const TIXATI: u8 = 15;
pub const WEBTORRENT: u8 = 16;
pub const DANPLAY: u8 = 17;
pub const KTORRENT: u8 = 18;
pub const RTORRENT: u8 = 19;
pub const ARES: u8 = 20;
pub const SHAREAZA: u8 = 21;
pub const TORRENT_STORM: u8 = 22;
pub const HALITE: u8 = 23;
pub const EBIT: u8 = 24;
pub const BITBUDDY: u8 = 25;
pub const BITLORD: u8 = 26;
pub const BITWOMBAT: u8 = 27;
pub const CTORRENT: u8 = 28;
pub const FILECROC: u8 = 29;
pub const FROSTWIRE: u8 = 30;
pub const GETRIGHT: u8 = 31;
pub const GSTORRENT: u8 = 32;
pub const GOPEED: u8 = 33;
pub const JSTORRENT: u8 = 34;
pub const KGET: u8 = 35;
pub const LEECHCRAFT: u8 = 36;
pub const LH_ABC: u8 = 37;
pub const LINKAGE: u8 = 38;
pub const LPHANT: u8 = 39;
pub const LIMEWIRE: u8 = 40;
pub const MONOTORRENT: u8 = 41;
pub const MEDIAGET: u8 = 42;
pub const PICO: u8 = 43;
pub const BITPI: u8 = 44;
pub const PHOENIX: u8 = 45;
pub const TORRENTVILLA: u8 = 46;
pub const MC: u8 = 47;
pub const BT_NEXT: u8 = 48;
pub const ONESWARM: u8 = 49;
pub const OMEGA: u8 = 50;
pub const CACHELOGIC: u8 = 51;
pub const POPCORN: u8 = 52;
pub const PANDO: u8 = 53;
pub const PEERPROJECT: u8 = 54;
pub const QQD: u8 = 55;
pub const RAIN: u8 = 56;
pub const RQ: u8 = 57;
pub const RUM: u8 = 58;
pub const RETRIEVER: u8 = 59;
pub const REZ: u8 = 60;
pub const SWIFTBIT: u8 = 61;
pub const GS_TORRENT: u8 = 62;
pub const SHARENET: u8 = 63;
pub const STELLAR: u8 = 64;
pub const TORRENT_GO: u8 = 65;
pub const TORRENT_NET: u8 = 66;
pub const TORRENT_STORM2: u8 = 67;
pub const TUOTU: u8 = 68;
pub const TTORRENT: u8 = 69;
pub const TORREX: u8 = 70;
pub const ULEECHER: u8 = 71;
pub const BITLET: u8 = 72;
pub const FIRETORRENT: u8 = 73;
pub const VAGAA: u8 = 74;
pub const XFPLAY: u8 = 75;
pub const XANTORRENT: u8 = 76;
pub const XTORRENT: u8 = 77;
pub const ZIPTORRENT: u8 = 78;
pub const ATORRENT: u8 = 79;
pub const ZONA: u8 = 80;
pub const BITSPIRIT: u8 = 81;
pub const AVICORA: u8 = 82;
pub const BITPUMP: u8 = 83;
pub const ADM: u8 = 84;
pub const AZTORRENT: u8 = 85;
pub const ENHANCED_CTORRENT: u8 = 86;
pub const DTORRENT: u8 = 87;
pub const PROPAGATE: u8 = 88;
pub const ELECTRIC_SHEEP: u8 = 89;
pub const FREEBOX: u8 = 90;
pub const FOXTORRENT: u8 = 91;
pub const FOLX: u8 = 92;
pub const NETDISK: u8 = 93;
pub const MIRO: u8 = 94;
pub const BITTORRENT_SDK: u8 = 95;
pub const BWS: u8 = 96;
pub const LIII: u8 = 97;
pub const MLDONKEY: u8 = 98;
pub const BM: u8 = 99;
pub const ALLPEERS: u8 = 100;
pub const QVOD: u8 = 101;
pub const ANIMEKO: u8 = 102;

/// Name for a client tag value. Returns "Unknown" for tag 0.
pub fn client_name(tag: u8) -> &'static str {
    match tag {
        QBITTORRENT => "qBittorrent",
        TRANSMISSION => "Transmission",
        UTORRENT => "uTorrent",
        BITTORRENT => "BitTorrent",
        LIBTORRENT => "libtorrent",
        BITCOMET => "BitComet",
        DELUGE => "Deluge",
        VUZE => "Vuze",
        BIGLYBT => "BiglyBT",
        XUNLEI => "Xunlei",
        XUNLEI_FAST => "Xunlei Fast",
        ARIA2 => "Aria2",
        FDM => "FDM",
        MAINLINE => "Mainline",
        TIXATI => "Tixati",
        WEBTORRENT => "WebTorrent",
        DANPLAY => "DanDanPlay",
        KTORRENT => "KTorrent",
        RTORRENT => "rTorrent",
        ARES => "Ares",
        SHAREAZA => "Shareaza",
        TORRENT_STORM => "TorrentStorm",
        HALITE => "Halite",
        EBIT => "EBit",
        BITBUDDY => "BitBuddy",
        BITLORD => "BitLord",
        BITWOMBAT => "BitWombat",
        CTORRENT => "CTorrent",
        FILECROC => "FileCroc",
        FROSTWIRE => "FrostWire",
        GETRIGHT => "GetRight",
        GSTORRENT => "GSTorrent",
        GOPEED => "Gopeed",
        JSTORRENT => "JSTorrent",
        KGET => "KGet",
        LEECHCRAFT => "LeechCraft",
        LH_ABC => "LH-ABC",
        LINKAGE => "linkage",
        LPHANT => "Lphant",
        LIMEWIRE => "LimeWire",
        MONOTORRENT => "MonoTorrent",
        MEDIAGET => "MediaGet",
        PICO => "picotorrent",
        BITPI => "Bitpirit",
        PHOENIX => "pHoeniX",
        TORRENTVILLA => "Torrentvilla",
        MC => "MC",
        BT_NEXT => "BT Next",
        ONESWARM => "OneSwarm",
        OMEGA => "OmegaTorrent",
        CACHELOGIC => "CacheLogic",
        POPCORN => "Popcorn Time",
        PANDO => "Pando",
        PEERPROJECT => "PeerProject",
        QQD => "QQ旋风",
        RAIN => "Rain",
        RQ => "rQ",
        RUM => "RUM Torrent",
        RETRIEVER => "Retriever",
        REZ => "RezTorrent",
        SWIFTBIT => "SwiftBit",
        GS_TORRENT => "GS Torrent",
        SHARENET => "ShareNET",
        STELLAR => "StellarPlayer",
        TORRENT_GO => "Torrent GO",
        TORRENT_NET => "Torrent.NET",
        TORRENT_STORM2 => "TorrentStorm",
        TUOTU => "TuoTu",
        TTORRENT => "tTorrent",
        TORREX => "Torrex Pro",
        ULEECHER => "uLeecher",
        BITLET => "Bitlet",
        FIRETORRENT => "FireTorrent",
        VAGAA => "Vagaa",
        XFPLAY => "Xfplay",
        XANTORRENT => "XanTorrent",
        XTORRENT => "XTorrent",
        ZIPTORRENT => "ZipTorrent",
        ATORRENT => "aTorrent",
        ZONA => "Zona",
        BITSPIRIT => "BitSpirit",
        AVICORA => "Avicora",
        BITPUMP => "BitPump",
        ADM => "ADM",
        AZTORRENT => "AzTorrent",
        ENHANCED_CTORRENT => "Enhanced CTorrent",
        DTORRENT => "DTorrent",
        PROPAGATE => "Propagate",
        ELECTRIC_SHEEP => "Electric Sheep",
        FREEBOX => "Freebox",
        FOXTORRENT => "FoxTorrent",
        FOLX => "folx",
        NETDISK => "netdisk",
        MIRO => "Miro",
        BITTORRENT_SDK => "BT SDK",
        BWS => "BitsOnWheels",
        LIII => "LIII",
        MLDONKEY => "MLDonkey",
        BM => "BitMagnet",
        ALLPEERS => "AllPeers",
        QVOD => "QVOD",
        ANIMEKO => "Animeko",
        _ => "Unknown",
    }
}

// ── Azureus-style lookup table (-XX...) ──────────────────────────────────────
// Indexed as AZUREUS_TABLE[peer_id[1] as usize][peer_id[2] as usize].
// Built at compile time; 64 KB, read-only at runtime.

const fn build_azureus_table() -> [[u8; 256]; 256] {
    let mut t = [[0u8; 256]; 256];

    // Ares: -A~, -AG, -AN, -AR
    t[b'A' as usize][b'~' as usize] = ARES;
    t[b'A' as usize][b'G' as usize] = ARES;
    t[b'A' as usize][b'N' as usize] = ARES;
    t[b'A' as usize][b'R' as usize] = ARES;
    // Animeko (AniLibtorrent)
    t[b'A' as usize][b'L' as usize] = ANIMEKO;
    // Avicora
    t[b'A' as usize][b'V' as usize] = AVICORA;
    // BitPump
    t[b'A' as usize][b'X' as usize] = BITPUMP;
    // ADM
    t[b'A' as usize][b'D' as usize] = ADM;
    // AzTorrent
    t[b'A' as usize][b'T' as usize] = AZTORRENT;
    // Vuze
    t[b'A' as usize][b'Z' as usize] = VUZE;
    // BitBuddy
    t[b'B' as usize][b'B' as usize] = BITBUDDY;
    // BitComet
    t[b'B' as usize][b'C' as usize] = BITCOMET;
    // BT SDK
    t[b'B' as usize][b'E' as usize] = BITTORRENT_SDK;
    // BiglyBT
    t[b'B' as usize][b'I' as usize] = BIGLYBT;
    // BitLord
    t[b'B' as usize][b'L' as usize] = BITLORD;
    // BitTorrent
    t[b'B' as usize][b'T' as usize] = BITTORRENT;
    // BitWombat
    t[b'B' as usize][b'W' as usize] = BITWOMBAT;
    // Shareaza Plus
    t[b'C' as usize][b'B' as usize] = SHAREAZA;
    // Enhanced CTorrent
    t[b'C' as usize][b'D' as usize] = ENHANCED_CTORRENT;
    // CTorrent
    t[b'C' as usize][b'T' as usize] = CTORRENT;
    // Deluge
    t[b'D' as usize][b'E' as usize] = DELUGE;
    // DTorrent (malicious)
    t[b'D' as usize][b'T' as usize] = DTORRENT;
    // Propagate
    t[b'D' as usize][b'P' as usize] = PROPAGATE;
    // DanDanPlay
    t[b'D' as usize][b'L' as usize] = DANPLAY;
    // EBit
    t[b'E' as usize][b'B' as usize] = EBIT;
    // Electric Sheep
    t[b'E' as usize][b'S' as usize] = ELECTRIC_SHEEP;
    // FlashGet
    t[b'F' as usize][b'G' as usize] = VAGAA; // leecher category
                                             // FileCroc
    t[b'F' as usize][b'C' as usize] = FILECROC;
    // FrostWire
    t[b'F' as usize][b'W' as usize] = FROSTWIRE;
    // Freebox
    t[b'F' as usize][b'X' as usize] = FREEBOX;
    // FoxTorrent
    t[b'F' as usize][b'T' as usize] = FOXTORRENT;
    // folx
    t[b'F' as usize][b'L' as usize] = FOLX;
    // GetRight
    t[b'G' as usize][b'R' as usize] = GETRIGHT;
    // GSTorrent
    t[b'G' as usize][b'S' as usize] = GSTORRENT;
    // Gopeed
    t[b'G' as usize][b'P' as usize] = GOPEED;
    // Halite
    t[b'H' as usize][b'L' as usize] = HALITE;
    // JSTorrent
    t[b'J' as usize][b'S' as usize] = JSTORRENT;
    // KGet
    t[b'K' as usize][b'G' as usize] = KGET;
    // KTorrent
    t[b'K' as usize][b'T' as usize] = KTORRENT;
    // LeechCraft
    t[b'L' as usize][b'C' as usize] = LEECHCRAFT;
    // LH-ABC
    t[b'L' as usize][b'H' as usize] = LH_ABC;
    // linkage
    t[b'L' as usize][b'K' as usize] = LINKAGE;
    // Lphant
    t[b'L' as usize][b'P' as usize] = LPHANT;
    // libtorrent
    t[b'L' as usize][b'T' as usize] = LIBTORRENT;
    // Torrentvilla
    t[b'l' as usize][b'r' as usize] = TORRENTVILLA;
    // LimeWire
    t[b'L' as usize][b'W' as usize] = LIMEWIRE;
    // MonoTorrent
    t[b'M' as usize][b'O' as usize] = MONOTORRENT;
    // MC
    t[b'M' as usize][b'C' as usize] = MC;
    // MediaGet2
    t[b'M' as usize][b'G' as usize] = MEDIAGET;
    // Miro
    t[b'M' as usize][b'R' as usize] = MIRO;
    // BT Next
    t[b'N' as usize][b'E' as usize] = BT_NEXT;
    // Net Transport
    t[b'N' as usize][b'X' as usize] = VAGAA; // leecher category
                                             // OneSwarm
    t[b'O' as usize][b'S' as usize] = ONESWARM;
    // OmegaTorrent
    t[b'O' as usize][b'T' as usize] = OMEGA;
    // CacheLogic
    t[b'P' as usize][b'C' as usize] = CACHELOGIC;
    // Popcorn Time
    t[b'P' as usize][b'T' as usize] = POPCORN;
    // Pando
    t[b'P' as usize][b'D' as usize] = PANDO;
    // PeerProject
    t[b'P' as usize][b'E' as usize] = PEERPROJECT;
    // picotorrent
    t[b'P' as usize][b'I' as usize] = PICO;
    // BitSpirit
    t[b'S' as usize][b'P' as usize] = BITSPIRIT;
    // pHoeniX
    t[b'p' as usize][b'X' as usize] = PHOENIX;
    // qBittorrent
    t[b'q' as usize][b'B' as usize] = QBITTORRENT;
    // QQ旋风
    t[b'Q' as usize][b'D' as usize] = QQD;
    // Rain
    t[b'R' as usize][b'N' as usize] = RAIN;
    // rQ
    t[b'R' as usize][b'Q' as usize] = RQ;
    // RUM Torrent
    t[b'R' as usize][b'M' as usize] = RUM;
    // Retriever
    t[b'R' as usize][b'T' as usize] = RTORRENT;
    // RezTorrent
    t[b'R' as usize][b'Z' as usize] = REZ;
    // Shareaza alpha/beta
    t[b'S' as usize][b'~' as usize] = SHAREAZA;
    // SwiftBit
    t[b'S' as usize][b'B' as usize] = SWIFTBIT;
    // Xunlei Fast
    t[b'S' as usize][b'D' as usize] = XUNLEI_FAST;
    // GS Torrent
    t[b'S' as usize][b'G' as usize] = GS_TORRENT;
    // ShareNET
    t[b'S' as usize][b'N' as usize] = SHARENET;
    // Shareaza
    t[b'S' as usize][b'Z' as usize] = SHAREAZA;
    // Torrent GO
    t[b'T' as usize][b'G' as usize] = TORRENT_GO;
    // Torrent.NET
    t[b'T' as usize][b'N' as usize] = TORRENT_NET;
    // Transmission
    t[b'T' as usize][b'R' as usize] = TRANSMISSION;
    // TorrentStorm
    t[b'T' as usize][b'S' as usize] = TORRENT_STORM;
    // TuoTu
    t[b'T' as usize][b'T' as usize] = TUOTU;
    // tTorrent
    t[b't' as usize][b'T' as usize] = TTORRENT;
    // Torrex Pro
    t[b'T' as usize][b'X' as usize] = TORREX;
    // uLeecher
    t[b'U' as usize][b'L' as usize] = ULEECHER;
    // µTorrent Embedded
    t[b'U' as usize][b'E' as usize] = UTORRENT;
    // µTorrent
    t[b'U' as usize][b'T' as usize] = UTORRENT;
    // µTorrent Mac
    t[b'U' as usize][b'M' as usize] = UTORRENT;
    // µTorrent Web
    t[b'U' as usize][b'W' as usize] = UTORRENT;
    // WebTorrent Desktop
    t[b'W' as usize][b'D' as usize] = WEBTORRENT;
    // Bitlet
    t[b'W' as usize][b'T' as usize] = BITLET;
    // WebTorrent
    t[b'W' as usize][b'W' as usize] = WEBTORRENT;
    // FireTorrent
    t[b'W' as usize][b'Y' as usize] = FIRETORRENT;
    // Vagaa
    t[b'V' as usize][b'G' as usize] = VAGAA;
    // Xfplay
    t[b'X' as usize][b'F' as usize] = XFPLAY;
    // Xunlei
    t[b'X' as usize][b'L' as usize] = XUNLEI;
    // XanTorrent
    t[b'X' as usize][b'T' as usize] = XANTORRENT;
    // XTorrent (-XX)
    t[b'X' as usize][b'X' as usize] = XTORRENT;
    // XTorrent (-XC)
    t[b'X' as usize][b'C' as usize] = XTORRENT;
    // ZipTorrent
    t[b'Z' as usize][b'T' as usize] = ZIPTORRENT;
    // aTorrent
    t[b'7' as usize][b'T' as usize] = ATORRENT;
    // Zona
    t[b'Z' as usize][b'O' as usize] = ZONA;
    // DTorrent (-4j)
    t[b'4' as usize][b'j' as usize] = DTORRENT;
    // LIII (-53)
    t[b'5' as usize][b'3' as usize] = LIII;
    // BitsOnWheels (-BOW maps to B,O)
    t[b'B' as usize][b'O' as usize] = BWS;
    // MLDonkey (-ML)
    t[b'M' as usize][b'L' as usize] = MLDONKEY;
    // MediaGet (-MG1 maps to M,G → same as MediaGet2)
    // MediaGet (-MG21 maps to M,G → same)
    // QVOD (QVOD peer_id: Q,V at [1],[2])
    t[b'Q' as usize][b'V' as usize] = QVOD;

    t
}

static AZUREUS_TABLE: [[u8; 256]; 256] = build_azureus_table();

// ── Non-Azureus prefix matching ──────────────────────────────────────────────
// Sorted by first byte for efficient scanning.
// (prefix_bytes, client_tag)

static NON_AZUREUS: &[(&[u8], u8)] = &[
    (b"A2-", ARIA2),
    (b"BM", BM),
    (b"btfans", SHAREAZA), // SimpleBT / BitComet old name
    (b"FD6", FDM),
    (b"M6-", MAINLINE),
    (b"M7-", MAINLINE),
    (b"M8-", MAINLINE),
    (b"MG-", MEDIAGET),
    (b"net", NETDISK),
    (b"TIX", TIXATI),
];

/// Identify the BitTorrent client from a peer_id.
/// Returns a client tag byte (0 = unknown).
pub fn identify(peer_id: &[u8; 20]) -> u8 {
    if peer_id[0] == b'-' {
        // Azureus-style: -XX...
        return AZUREUS_TABLE[peer_id[1] as usize][peer_id[2] as usize];
    }

    // Try non-Azureus prefixes (sorted by first byte)
    for &(prefix, tag) in NON_AZUREUS {
        let len = prefix.len();
        if len <= 20 && peer_id[..len] == *prefix {
            return tag;
        }
    }

    UNKNOWN
}
