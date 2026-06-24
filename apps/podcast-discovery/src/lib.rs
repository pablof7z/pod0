//! `podcast-discovery` — NIP-F4 podcast discovery layer.
//!
//! Parses Nostr `kind:10154` / `kind:54` events into raw NIP-F4 views
//! ([`NipF4DiscoveryShow`], [`NipF4DiscoveryEpisode`]) and the `podcast-core` domain rows
//! they map onto. Also builds the tag sets the kernel-side publisher
//! consumes when republishing or creating new shows/episodes.
//!
//! NIP-F4 uses per-podcast keypairs:
//!   - kind:10154 — show metadata (replaceable per podcast pubkey, no d-tag)
//!   - kind:54    — episode events (regular events, no d-tag, no a-tag)
//!   - kind:10064 — author claim (agent key declares ownership of podcast keys)
//!
//! ## Scope (M10.A)
//!
//! * Schema constants in [`kinds`] — pinned per NIP-F4 protocol spec.
//! * Parse layer ([`parse_show_event`], [`parse_episode_event`]) — total
//!   functions that turn raw `Vec<Vec<String>>` tags into typed views.
//! * Build layer ([`podcast_to_show_tags`], [`episode_to_episode_tags`])
//!   — produce the tag list a NIP-F4 publisher signs.
//! * Domain mapping ([`show_to_podcast`], [`episode_to_episode`]) —
//!   takes the parsed view onto the `podcast_core` domain types so the
//!   discovery flow lands a usable `Podcast` row in `LibraryProjection`.
//! * Action ids ([`actions`]) — stable wire strings the iOS layer
//!   encodes against, no kernel coupling yet.
//!
//! ## Out of scope (later units)
//!
//! * No relay I/O. Subscribing, REQ/EOSE handling, and outbox writes
//!   land in M10.D (`podcast-discovery::nostr` orchestration on top of
//!   the future `nmp-nip-f4` NMP crate).
//! * No Blossom upload. `ImetaInfo` is the seam for the M10.B uploader
//!   to thread post-upload metadata into the event builder.
//!
//! ## Doctrine
//!
//! * **Pure** — no async, no I/O, no `nmp-core` deps. Tests drive every
//!   parse + build path deterministically.
//! * **No `nostr` crate dep** — we work directly off `Vec<Vec<String>>`
//!   tags as delivered by the NMP kernel. The kernel owns typed event
//!   reconstruction; this crate owns the NIP-F4 schema.
//! * **300 LOC soft / 500 LOC hard** per file (matches AGENTS.md).

pub mod actions;
pub mod build;
pub mod kinds;
pub mod nip_f4;
pub mod parse;
pub mod types;

pub use actions::{
    DiscoverPodcastsAction, PublishEpisodeAction, PublishShowAction, ACTION_DISCOVER_PODCASTS,
    ACTION_PUBLISH_EPISODE, ACTION_PUBLISH_SHOW,
};
pub use build::{
    episode_to_episode_tags, episode_to_episode_tags_with_imeta, podcast_to_show_tags,
    show_content, ImetaInfo,
};
pub use kinds::{KIND_AUTHOR_CLAIM, KIND_EPISODE, KIND_SHOW};
pub use nip_f4::{
    parse_event_json as parse_nip_f4_event_json, parse_kind_10154, parse_kind_54, NipF4Episode,
    NipF4Show, KIND_NIP_F4_AUTHOR_CLAIM, KIND_NIP_F4_EPISODE, KIND_NIP_F4_SHOW,
};
pub use parse::{episode_to_episode, parse_episode_event, parse_show_event, show_to_podcast};
pub use types::{NipF4DiscoveryEpisode, NipF4DiscoveryShow, ParseError, ShowReference};

#[cfg(test)]
mod round_trip_tests {
    use super::*;
    use podcast_core::types::podcast::{Podcast, PodcastId};
    use url::Url;
    use uuid::Uuid;

    /// Build → parse → re-map round trip preserves the load-bearing fields a
    /// discovery client cares about (title, description, image, language,
    /// categories, owner pubkey, coordinate).
    #[test]
    fn show_round_trip_preserves_load_bearing_fields() {
        let mut p = Podcast::new("Round-Trip Show");
        p.id = PodcastId::new(Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap());
        p.author = "Host".into();
        p.description = "Show description".into();
        p.image_url = Some(Url::parse("https://img.example/cover.jpg").unwrap());
        p.language = Some("en".into());
        p.categories = vec!["Technology".into(), "News".into()];

        let podcast_pk = "podcast-pubkey-hex";
        let tags = podcast_to_show_tags(&p, podcast_pk);
        let content = show_content(&p);

        let parsed =
            parse_show_event(KIND_SHOW, podcast_pk, 1_700_000_000, &content, &tags).expect("parse");
        assert_eq!(parsed.title, "Round-Trip Show");
        assert_eq!(parsed.description, "Show description");
        assert_eq!(parsed.image_url.as_deref(), Some("https://img.example/cover.jpg"));
        assert_eq!(parsed.language.as_deref(), Some("en"));
        assert_eq!(parsed.categories, vec!["Technology".to_string(), "News".into()]);
        assert_eq!(parsed.author_pubkey.as_deref(), Some(podcast_pk));

        // Re-mapping back to a `Podcast` keeps everything that matters.
        let p2 = show_to_podcast(&parsed);
        assert_eq!(p2.title, p.title);
        assert_eq!(p2.description, p.description);
        assert_eq!(p2.language, p.language);
        assert_eq!(p2.categories, p.categories);
        assert_eq!(p2.image_url, p.image_url);
        assert_eq!(p2.owner_pubkey_hex.as_deref(), Some(podcast_pk));
        // NIP-F4: coordinate is "10154:<podcast-pubkey>" — no d-tag.
        assert_eq!(
            p2.nostr_coordinate.as_deref(),
            Some("10154:podcast-pubkey-hex")
        );
    }

    /// Coordinate is stable for the same pubkey across builds.
    #[test]
    fn show_coordinate_is_stable_per_pubkey() {
        let p = Podcast::new("X");
        let tags = podcast_to_show_tags(&p, "stable-pk");
        let parsed_a = parse_show_event(KIND_SHOW, "stable-pk", 0, "", &tags).expect("parse a");
        let parsed_b = parse_show_event(KIND_SHOW, "stable-pk", 99, "", &tags).expect("parse b");
        assert_eq!(parsed_a.coordinate(), parsed_b.coordinate());
        assert_eq!(parsed_a.coordinate(), "10154:stable-pk");
    }
}
