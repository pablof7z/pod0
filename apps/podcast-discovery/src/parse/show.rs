//! Parse a `kind:10154` Nostr event (NIP-F4) into a [`NipF4DiscoveryShow`] and map it onto
//! a [`Podcast`].
//!
//! Behavior mirrors `App/Sources/Services/NostrPodcastDiscoveryService
//! .parseShow(from:)` so the Rust + Swift discovery paths converge on the
//! same `Podcast` row given identical wire input.

use podcast_core::types::podcast::{NostrVisibility, Podcast, PodcastId};
use url::Url;
use uuid::Uuid;

use crate::kinds::KIND_SHOW;
use crate::parse::{all_tag_values, first_tag_value};
use crate::types::{NipF4DiscoveryShow, ParseError};

/// Parse a Nostr event's header + tags into a raw [`NipF4DiscoveryShow`].
///
/// `kind` is checked against [`KIND_SHOW`]. The `pubkey` is the podcast's own
/// hex pubkey (NIP-F4 per-podcast key). `created_at` is the event header
/// timestamp (unix seconds). `content` is the event content string — used as a
/// fallback for `description` when no `["description", ...]` tag is present.
///
/// NIP-F4 shows have no `d` tag; the show is identified by pubkey alone.
pub fn parse_show_event(
    kind: u32,
    pubkey: &str,
    created_at: i64,
    content: &str,
    tags: &[Vec<String>],
) -> Result<NipF4DiscoveryShow, ParseError> {
    if kind != KIND_SHOW {
        return Err(ParseError::WrongKind {
            expected: KIND_SHOW,
            got: kind,
        });
    }

    // Title falls back to a prefix of `content` (mirrors Swift parseShow).
    let title = first_tag_value(tags, "title")
        .map(str::to_string)
        .or_else(|| {
            if content.is_empty() {
                None
            } else {
                Some(content.chars().take(80).collect())
            }
        })
        .ok_or(ParseError::MissingTag("title"))?;
    if title.is_empty() {
        return Err(ParseError::EmptyTag("title"));
    }

    let description = first_tag_value(tags, "description")
        .map(str::to_string)
        .unwrap_or_else(|| content.to_string());

    Ok(NipF4DiscoveryShow {
        pubkey: pubkey.to_string(),
        title,
        description,
        image_url: first_tag_value(tags, "image").map(str::to_string),
        language: first_tag_value(tags, "language").map(str::to_string),
        author_pubkey: first_tag_value(tags, "p").map(str::to_string),
        categories: all_tag_values(tags, "t"),
        created_at,
    })
}

/// Map a parsed [`NipF4DiscoveryShow`] onto a [`Podcast`] domain row.
///
/// The mapping is total: every field that does not parse as a URL is
/// silently dropped (matches Swift `URL(string:)` semantics, which is the
/// existing wire contract).
///
/// `Podcast::id` is a UUIDv5 derived from the NIP-F4 coordinate so the
/// row is stable across rediscoveries — `subscribe(to:)` in Swift uses
/// the same scheme via `NostrPodcastDiscoveryService.podcastID(for:)`.
pub fn show_to_podcast(show: &NipF4DiscoveryShow) -> Podcast {
    let coordinate = show.coordinate();
    Podcast {
        id: podcast_id_from_coordinate(&coordinate),
        feed_url: None,
        title: show.title.clone(),
        author: show.author_pubkey.clone().unwrap_or_default(),
        image_url: show.image_url.as_deref().and_then(|s| Url::parse(s).ok()),
        description: show.description.clone(),
        language: show.language.clone(),
        categories: show.categories.clone(),
        discovered_at: chrono::Utc::now(),
        owner_pubkey_hex: Some(show.pubkey.clone()),
        nostr_visibility: NostrVisibility::Public,
        nostr_coordinate: Some(coordinate),
        title_is_placeholder: false,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    }
}

/// UUIDv5 of the NIP-F4 coordinate string, using a project-scoped
/// namespace UUID so values can be replayed deterministically.
fn podcast_id_from_coordinate(coordinate: &str) -> PodcastId {
    const NS: Uuid = Uuid::from_bytes([
        0xd9, 0x7c, 0x4d, 0x7d, 0xa1, 0x12, 0x5b, 0x4f, 0x9a, 0x0b, 0x71, 0x12, 0xb6, 0x4c, 0xc3,
        0x2d,
    ]);
    PodcastId::new(Uuid::new_v5(&NS, coordinate.as_bytes()))
}

#[cfg(test)]
#[path = "show_tests.rs"]
mod tests;
