//! NIP-74 event kind numbers.
//!
//! Pinned in one place so the parse + build sides cannot drift. The kind
//! numbers are the protocol contract — changing them is a breaking wire
//! change that requires an ADR (see `Plans/nmp-migration/02-crates.md`
//! §B `nmp-nip74`).
//!
//! NIP-74 reuses NIP-33's parameterised-replaceable-event mechanism
//! (kinds 30000–39999): the `d` tag is the per-author stable identifier
//! and the latest event at `<author, kind, d>` wins.

/// Podcast show — kind:30074 (parameterised replaceable).
///
/// Tag layout per `App/Sources/Services/NostrPodcastPublisher.swift`:
/// `["d", "<uuid>"]`, `["title", ...]`, `["summary", ...]`, `["p", <pubkey>]`,
/// optional `["image", ...]`, `["language", ...]`, repeated `["t", <category>]`.
pub const KIND_SHOW: u32 = 30074;

/// Podcast episode — kind:30075 (parameterised replaceable).
///
/// Tag layout per `App/Sources/Services/NostrPodcastPublisher.swift`:
/// `["d", "<uuid>"]`, `["title", ...]`, `["published_at", <unix>]`,
/// `["a", "30074:<pubkey>:<show-d>"]`, optional `["summary", ...]`,
/// `["duration", <secs>]`, `["image", <url>]`,
/// `["imeta", "url <u>", "m <mime>", "x <sha256>", "size <bytes>", ...]`,
/// `["chapters", <url>, <mime>]`, `["transcript", <url>, <mime>]`.
pub const KIND_EPISODE: u32 = 30075;

/// Podcast-owner author claim — kind:10064 (replaceable).
///
/// Used by the NIP-F4 migration (see `docs/plan/pod0-nostr-publishing.md`)
/// to let an agent key declare ownership of a set of per-podcast pubkeys.
/// Not parsed in this crate yet — exported here so M10.D (publish
/// orchestration) and the future NIP-F4 cutover share one constant.
pub const KIND_AUTHOR_CLAIM: u32 = 10064;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_constants_match_swift_publisher() {
        // Mirrors NostrPodcastDiscoveryService.Wire.kindShow / kindEpisode
        // and NostrPodcastPublisher.publishShow/publishEpisode kind args.
        assert_eq!(KIND_SHOW, 30074);
        assert_eq!(KIND_EPISODE, 30075);
        assert_eq!(KIND_AUTHOR_CLAIM, 10064);
    }
}
