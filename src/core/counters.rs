//! Tracker-level incremental counters for O(1) snapshots.
//!
//! Swarm mutation methods return delta structs ([`PeerUpsert`], [`PeerRemoval`],
//! [`ExpireResult`]). The [`Tracker`](crate::core::tracker::Tracker) uses these to
//! maintain running totals in [`TrackerCounters`], eliminating the need to
//! iterate all swarms on every snapshot.

/// Incremental counters maintained at the [`Tracker`](crate::core::tracker::Tracker) level.
///
/// Updated by every mutation path (`announce`, `expire`) so that
/// [`snapshot()`](crate::core::tracker::Tracker::snapshot) can read them in O(1).
#[derive(Debug, Default)]
pub(crate) struct TrackerCounters {
    pub torrents: usize,
    pub peers: usize,
    pub seeders: usize,
    pub downloaded: u64,
}

/// Returned by [`Swarm::upsert_peer`](crate::core::tracker::Swarm::upsert_peer).
pub(crate) struct PeerUpsert {
    /// `true` if this peer did not exist before.
    pub is_new_peer: bool,
    /// `true` if the old peer (if any) was a seeder.
    pub was_complete: bool,
    /// `true` if the new peer state is seeder.
    pub now_complete: bool,
    /// Client tag of the old peer, if it existed.
    pub old_tag: Option<u8>,
}

/// Returned by [`Swarm::remove_peer_tag`](crate::core::tracker::Swarm::remove_peer_tag).
pub(crate) struct PeerRemoval {
    /// Client tag of the removed peer.
    pub tag: u8,
    /// `true` if the removed peer was a seeder.
    pub was_complete: bool,
}

/// Returned by [`Swarm::expire`](crate::core::tracker::Swarm::expire).
pub(crate) struct ExpireResult {
    /// Client tags of all expired peers.
    pub tags: Vec<u8>,
    /// Number of peers that expired.
    pub removed_peers: usize,
    /// Number of expired peers that were seeders.
    pub removed_complete: usize,
}

impl TrackerCounters {
    /// Apply the delta from a peer upsert (insert or update).
    #[inline]
    pub(crate) fn apply_upsert(&mut self, upsert: &PeerUpsert) {
        if upsert.is_new_peer {
            self.peers += 1;
        }
        if upsert.now_complete && !upsert.was_complete {
            self.seeders += 1;
        } else if !upsert.now_complete && upsert.was_complete {
            self.seeders = self.seeders.saturating_sub(1);
        }
    }

    /// Apply the delta from a peer removal.
    #[inline]
    pub(crate) fn apply_removal(&mut self, removal: &PeerRemoval) {
        self.peers = self.peers.saturating_sub(1);
        if removal.was_complete {
            self.seeders = self.seeders.saturating_sub(1);
        }
    }

    /// Apply the delta from an expire sweep.
    #[inline]
    pub(crate) fn apply_expire(
        &mut self,
        result: &ExpireResult,
        removed_swarms: usize,
        removed_downloaded: u64,
    ) {
        self.peers = self.peers.saturating_sub(result.removed_peers);
        self.seeders = self.seeders.saturating_sub(result.removed_complete);
        self.torrents = self.torrents.saturating_sub(removed_swarms);
        self.downloaded = self.downloaded.saturating_sub(removed_downloaded);
    }

    /// Record a new torrent (swarm created).
    #[inline]
    pub(crate) fn add_torrent(&mut self) {
        self.torrents += 1;
    }

    /// Record a new completion event.
    #[inline]
    pub(crate) fn add_downloaded(&mut self) {
        self.downloaded += 1;
    }

    /// Verify counters match a full traversal (debug builds only).
    #[cfg(debug_assertions)]
    pub(crate) fn verify(&self, torrents: usize, peers: usize, seeders: usize, downloaded: u64) {
        assert_eq!(self.torrents, torrents, "torrents counter drift");
        assert_eq!(self.peers, peers, "peers counter drift");
        assert_eq!(self.seeders, seeders, "seeders counter drift");
        assert_eq!(self.downloaded, downloaded, "downloaded counter drift");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_new_seeder() {
        let mut c = TrackerCounters::default();
        c.apply_upsert(&PeerUpsert {
            is_new_peer: true,
            was_complete: false,
            now_complete: true,
            old_tag: None,
        });
        assert_eq!(c.peers, 1);
        assert_eq!(c.seeders, 1);
    }

    #[test]
    fn upsert_new_leecher() {
        let mut c = TrackerCounters::default();
        c.apply_upsert(&PeerUpsert {
            is_new_peer: true,
            was_complete: false,
            now_complete: false,
            old_tag: None,
        });
        assert_eq!(c.peers, 1);
        assert_eq!(c.seeders, 0);
    }

    #[test]
    fn upsert_leecher_to_seeder() {
        let mut c = TrackerCounters {
            torrents: 1,
            peers: 1,
            seeders: 0,
            downloaded: 0,
        };
        c.apply_upsert(&PeerUpsert {
            is_new_peer: false,
            was_complete: false,
            now_complete: true,
            old_tag: Some(1),
        });
        assert_eq!(c.peers, 1);
        assert_eq!(c.seeders, 1);
    }

    #[test]
    fn upsert_seeder_to_leecher() {
        let mut c = TrackerCounters {
            torrents: 1,
            peers: 1,
            seeders: 1,
            downloaded: 0,
        };
        c.apply_upsert(&PeerUpsert {
            is_new_peer: false,
            was_complete: true,
            now_complete: false,
            old_tag: Some(1),
        });
        assert_eq!(c.peers, 1);
        assert_eq!(c.seeders, 0);
    }

    #[test]
    fn removal_seeder() {
        let mut c = TrackerCounters {
            torrents: 1,
            peers: 2,
            seeders: 1,
            downloaded: 0,
        };
        c.apply_removal(&PeerRemoval {
            tag: 0,
            was_complete: true,
        });
        assert_eq!(c.peers, 1);
        assert_eq!(c.seeders, 0);
    }

    #[test]
    fn removal_leecher() {
        let mut c = TrackerCounters {
            torrents: 1,
            peers: 2,
            seeders: 1,
            downloaded: 0,
        };
        c.apply_removal(&PeerRemoval {
            tag: 0,
            was_complete: false,
        });
        assert_eq!(c.peers, 1);
        assert_eq!(c.seeders, 1);
    }

    #[test]
    fn expire_mixed() {
        let mut c = TrackerCounters {
            torrents: 3,
            peers: 10,
            seeders: 4,
            downloaded: 50,
        };
        c.apply_expire(
            &ExpireResult {
                tags: vec![],
                removed_peers: 3,
                removed_complete: 1,
            },
            1, // removed_swarms
            5, // removed_downloaded
        );
        assert_eq!(c.torrents, 2);
        assert_eq!(c.peers, 7);
        assert_eq!(c.seeders, 3);
        assert_eq!(c.downloaded, 45);
    }

    #[test]
    fn saturating_on_underflow() {
        let mut c = TrackerCounters {
            torrents: 0,
            peers: 0,
            seeders: 0,
            downloaded: 0,
        };
        c.apply_removal(&PeerRemoval {
            tag: 0,
            was_complete: true,
        });
        assert_eq!(c.peers, 0);
        assert_eq!(c.seeders, 0);
    }
}
