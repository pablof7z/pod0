//! Build the tag set for a `kind:30074` show event from a [`Podcast`].
//!
//! Port of `NostrPodcastPublisher.publishShow` — the iOS publisher sets
//! the `["d", "podcast:guid:<uuid>"]` prefix and emits tags in a specific
//! order. The order is part of the wire contract because some relays
//! (and the Swift discovery service) prefer the first match per tag
//! name; preserving it keeps round-trips deterministic.

use podcast_core::types::podcast::Podcast;

/// Prefix used for the show `d` tag — matches Swift
/// `"podcast:guid:\(podcast.id.uuidString.lowercased())"`. Kept private
/// so callers go through [`podcast_to_show_tags`] / [`show_d_tag`] and
/// the prefix lives in one place.
const SHOW_D_PREFIX: &str = "podcast:guid:";

/// Build the canonical `d` tag value for a podcast.
pub fn show_d_tag(podcast: &Podcast) -> String {
    format!("{SHOW_D_PREFIX}{}", podcast.id.0.simple().to_string().to_ascii_lowercase())
}

/// The string emitted as the event's `content` field.
///
/// The Swift publisher passes `podcast.description` as content; we mirror
/// that so other Nostr clients (which read `content` rather than the
/// `summary` tag) see the same description text.
pub fn show_content(podcast: &Podcast) -> String {
    podcast.description.clone()
}

/// Build the tag list for a `kind:30074` show event.
///
/// `agent_pubkey` is the hex pubkey of the signer (the agent key that
/// publishes the show). It's threaded explicitly because this function
/// is pure data — the signer is owned by the kernel-side action module.
pub fn podcast_to_show_tags(podcast: &Podcast, agent_pubkey: &str) -> Vec<Vec<String>> {
    let mut tags: Vec<Vec<String>> = vec![
        vec!["d".into(), show_d_tag(podcast)],
        vec!["title".into(), podcast.title.clone()],
    ];
    if !podcast.description.is_empty() {
        tags.push(vec!["summary".into(), podcast.description.clone()]);
    }
    if !podcast.author.is_empty() {
        tags.push(vec!["p".into(), agent_pubkey.to_string()]);
    }
    if let Some(image) = &podcast.image_url {
        tags.push(vec!["image".into(), image.as_str().to_string()]);
    }
    if let Some(lang) = &podcast.language {
        if !lang.is_empty() {
            tags.push(vec!["language".into(), lang.clone()]);
        }
    }
    for category in &podcast.categories {
        tags.push(vec!["t".into(), category.clone()]);
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::types::podcast::{Podcast, PodcastId};
    use url::Url;
    use uuid::Uuid;

    fn fixture() -> Podcast {
        let mut p = Podcast::new("My Show");
        p.id = PodcastId::new(Uuid::parse_str("12345678-1234-1234-1234-1234567890ab").unwrap());
        p.author = "Host".into();
        p.description = "A great show".into();
        p.image_url = Some(Url::parse("https://img.example/cover.jpg").unwrap());
        p.language = Some("en".into());
        p.categories = vec!["Technology".into(), "News".into()];
        p
    }

    #[test]
    fn d_tag_is_lowercase_uuid_with_prefix() {
        let p = fixture();
        assert_eq!(
            show_d_tag(&p),
            "podcast:guid:123456781234123412341234567890ab"
        );
    }

    #[test]
    fn minimal_show_emits_d_and_title_only() {
        let p = Podcast::new("Title Only");
        let tags = podcast_to_show_tags(&p, "agent-pk");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0][0], "d");
        assert_eq!(tags[1], vec!["title".to_string(), "Title Only".into()]);
    }

    #[test]
    fn full_show_emits_every_tag_in_publisher_order() {
        let p = fixture();
        let tags = podcast_to_show_tags(&p, "agent-pk");
        let names: Vec<&str> = tags.iter().filter_map(|t| t.first().map(String::as_str)).collect();
        assert_eq!(
            names,
            vec!["d", "title", "summary", "p", "image", "language", "t", "t"]
        );
        assert_eq!(tags[3], vec!["p".to_string(), "agent-pk".into()]);
        assert_eq!(tags[6], vec!["t".to_string(), "Technology".into()]);
        assert_eq!(tags[7], vec!["t".to_string(), "News".into()]);
    }

    #[test]
    fn show_content_uses_podcast_description() {
        assert_eq!(show_content(&fixture()), "A great show");
        assert_eq!(show_content(&Podcast::new("Empty")), "");
    }
}
