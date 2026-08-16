use std::collections::HashSet;

use pod0_domain::{PodcastId, UnixTimestampMilliseconds};
use sha2::{Digest, Sha256};

use crate::{
    CatalogEpisodeCandidate, FeedParseFailure, ResolvedSharedEpisode, normalize_feed_url,
    parse_podcast_feed,
};

pub fn catalog_candidates_from_feed(
    bytes: &[u8],
    response_url: &str,
    episode_query: &str,
    podcast_hint: Option<&str>,
    observed_at_ms: i64,
) -> Result<Vec<CatalogEpisodeCandidate>, FeedParseFailure> {
    let identity = normalize_feed_url(response_url).ok_or(FeedParseFailure::InvalidUrl)?;
    let podcast_id = stable_podcast_id(&identity.comparison_key);
    let parsed = parse_podcast_feed(
        bytes,
        identity.clone(),
        podcast_id,
        UnixTimestampMilliseconds::new(observed_at_ms),
    )?;
    let show_score = show_score(&parsed.podcast.title, &parsed.podcast.author, podcast_hint);
    Ok(parsed
        .episodes
        .into_iter()
        .filter_map(|episode| {
            let score =
                episode_score(&episode.title, &episode.description, episode_query)? + show_score;
            Some(CatalogEpisodeCandidate {
                episode: ResolvedSharedEpisode {
                    podcast_id,
                    podcast_title: parsed.podcast.title.clone(),
                    feed_url: Some(identity.source_url.clone()),
                    audio_url: episode.enclosure_url,
                    guid: Some(episode.publisher_guid),
                    title: episode.title,
                    description: episode.description,
                    published_at_milliseconds: episode.published_at.value,
                    enclosure_mime_type: episode.enclosure_mime_type,
                    image_url: episode
                        .image_url
                        .or_else(|| parsed.podcast.image_url.clone()),
                    duration_milliseconds: episode.duration_milliseconds,
                },
                score,
            })
        })
        .collect())
}

#[must_use]
pub fn select_catalog_candidates(
    mut candidates: Vec<CatalogEpisodeCandidate>,
    limit: u16,
) -> Vec<CatalogEpisodeCandidate> {
    candidates.sort_by(|left, right| {
        right.score.cmp(&left.score).then_with(|| {
            right
                .episode
                .published_at_milliseconds
                .cmp(&left.episode.published_at_milliseconds)
        })
    });
    candidates.truncate(usize::from(limit.clamp(1, 10)));
    candidates
}

fn episode_score(title: &str, description: &str, query: &str) -> Option<u32> {
    let query = normalize(query);
    if query.is_empty() {
        return None;
    }
    let title = normalize(title);
    let description = normalize(description);
    let wanted = tokens(&query);
    let title_hits = wanted.intersection(&tokens(&title)).count() as u32;
    let description_hits = wanted.intersection(&tokens(&description)).count() as u32;
    let phrase = u32::from(title.contains(&query) || query.contains(&title)) * 80;
    let score = phrase + title_hits * 18 + description_hits * 3;
    (score > 0).then_some(score)
}

fn show_score(title: &str, author: &str, hint: Option<&str>) -> u32 {
    let Some(hint) = hint.map(normalize).filter(|value| !value.is_empty()) else {
        return 0;
    };
    let show = normalize(&format!("{title} {author}"));
    let hits = tokens(&hint).intersection(&tokens(&show)).count() as u32;
    u32::from(normalize(title).contains(&hint)) * 100 + hits * 20
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|value| if value.is_alphanumeric() { value } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(value: &str) -> HashSet<String> {
    const IGNORED: &[&str] = &[
        "a", "an", "and", "for", "from", "in", "of", "on", "the", "to", "with",
    ];
    value
        .split_whitespace()
        .filter(|value| !IGNORED.contains(value))
        .map(str::to_owned)
        .collect()
}

fn stable_podcast_id(value: &str) -> PodcastId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/shared-podcast/v1\0");
    hash.update(value.to_lowercase());
    let value: [u8; 32] = hash.finalize().into();
    PodcastId::from_bytes(value[..16].try_into().expect("fixed digest"))
}
