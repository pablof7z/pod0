use pod0_domain::{PodcastId, UnixTimestampMilliseconds};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{
    EpisodeWebPageMetadata, FeedParseFailure, LibraryHttpRequest, ResolvedSharedEpisode,
    normalize_feed_url, parse_podcast_feed,
};

#[must_use]
pub fn plan_shared_apple_lookup(identifier: &str) -> Option<LibraryHttpRequest> {
    let identifier = identifier.trim();
    if identifier.is_empty() || !identifier.bytes().all(|value| value.is_ascii_digit()) {
        return None;
    }
    let mut url = Url::parse("https://itunes.apple.com/lookup").ok()?;
    url.query_pairs_mut()
        .append_pair("id", identifier)
        .append_pair("entity", "podcast");
    Some(LibraryHttpRequest {
        url: url.into(),
        accept: "application/json".into(),
        maximum_response_bytes: 1_000_000,
    })
}

#[must_use]
pub fn plan_shared_feed_request(feed_url: &str) -> Option<LibraryHttpRequest> {
    let identity = normalize_feed_url(feed_url)?;
    Some(LibraryHttpRequest {
        url: identity.source_url,
        accept: "application/rss+xml, application/atom+xml;q=0.9, application/xml;q=0.8".into(),
        maximum_response_bytes: 10_000_000,
    })
}

pub fn parse_shared_lookup_feed_url(bytes: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        feed_url: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Response {
        results: Vec<Row>,
    }
    serde_json::from_slice::<Response>(bytes)
        .ok()?
        .results
        .into_iter()
        .find_map(|row| row.feed_url)
        .and_then(|value| normalize_feed_url(&value).map(|identity| identity.source_url))
}

#[must_use]
pub fn direct_shared_episode(source_url: &str, now_ms: i64) -> Option<ResolvedSharedEpisode> {
    let url = Url::parse(source_url).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !looks_like_audio(&url) {
        return None;
    }
    Some(ResolvedSharedEpisode {
        podcast_id: stable_podcast_id(&format!("synthetic:{}", url.host_str()?)),
        podcast_title: url.host_str()?.to_owned(),
        feed_url: None,
        audio_url: without_fragment(&url).into(),
        guid: None,
        title: fallback_title(&url),
        description: String::new(),
        published_at_milliseconds: now_ms,
        enclosure_mime_type: mime_type(&url),
        image_url: None,
        duration_milliseconds: None,
    })
}

#[must_use]
pub fn page_direct_episode(
    page: &EpisodeWebPageMetadata,
    response_url: &str,
    now_ms: i64,
) -> Option<ResolvedSharedEpisode> {
    let audio_url = page.audio_url.as_deref()?;
    let source = Url::parse(response_url).ok()?;
    let title_url = page
        .canonical_url
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
        .unwrap_or_else(|| source.clone());
    let podcast_title = page
        .podcast_title
        .clone()
        .or_else(|| source.host_str().map(str::to_owned))
        .unwrap_or_else(|| "Shared Podcast".into());
    Some(ResolvedSharedEpisode {
        podcast_id: page
            .feed_url
            .as_deref()
            .map(stable_podcast_id)
            .unwrap_or_else(|| {
                stable_podcast_id(&format!("synthetic:{}", podcast_title.to_lowercase()))
            }),
        podcast_title,
        feed_url: page.feed_url.clone(),
        audio_url: without_fragment(&Url::parse(audio_url).ok()?).into(),
        guid: page.guid.clone(),
        title: page
            .episode_title
            .clone()
            .unwrap_or_else(|| fallback_title(&title_url)),
        description: page.description.clone().unwrap_or_default(),
        published_at_milliseconds: page.published_at_milliseconds.unwrap_or(now_ms),
        enclosure_mime_type: page.audio_mime_type.clone(),
        image_url: page.image_url.clone(),
        duration_milliseconds: page.duration_milliseconds,
    })
}

pub fn resolve_shared_episode_from_feed(
    bytes: &[u8],
    response_url: &str,
    page: &EpisodeWebPageMetadata,
    now_ms: i64,
) -> Result<ResolvedSharedEpisode, FeedParseFailure> {
    let identity = normalize_feed_url(response_url).ok_or(FeedParseFailure::InvalidUrl)?;
    let podcast_id = stable_podcast_id(&identity.comparison_key);
    let parsed = parse_podcast_feed(
        bytes,
        identity.clone(),
        podcast_id,
        UnixTimestampMilliseconds::new(now_ms),
    )?;
    let episode = best_match(&parsed.episodes, page).ok_or(FeedParseFailure::MissingChannel)?;
    Ok(ResolvedSharedEpisode {
        podcast_id,
        podcast_title: if parsed.podcast.title.is_empty() {
            page.podcast_title
                .clone()
                .unwrap_or_else(|| "Shared Podcast".into())
        } else {
            parsed.podcast.title
        },
        feed_url: Some(identity.source_url),
        audio_url: episode.enclosure_url.clone(),
        guid: Some(episode.publisher_guid.clone()),
        title: if episode.title.is_empty() {
            page.episode_title
                .clone()
                .unwrap_or_else(|| "Shared Episode".into())
        } else {
            episode.title.clone()
        },
        description: if episode.description.is_empty() {
            page.description.clone().unwrap_or_default()
        } else {
            episode.description.clone()
        },
        published_at_milliseconds: episode.published_at.value,
        enclosure_mime_type: episode
            .enclosure_mime_type
            .clone()
            .or_else(|| page.audio_mime_type.clone()),
        image_url: episode
            .image_url
            .clone()
            .or_else(|| page.image_url.clone())
            .or(parsed.podcast.image_url),
        duration_milliseconds: episode.duration_milliseconds.or(page.duration_milliseconds),
    })
}

fn best_match<'a>(
    episodes: &'a [pod0_domain::EpisodeRecord],
    page: &EpisodeWebPageMetadata,
) -> Option<&'a pod0_domain::EpisodeRecord> {
    if let Some(audio) = &page.audio_url {
        let wanted = comparable_url(audio);
        if let Some(value) = episodes
            .iter()
            .find(|episode| comparable_url(&episode.enclosure_url) == wanted)
        {
            return Some(value);
        }
    }
    if let Some(guid) = &page.guid
        && let Some(value) = episodes
            .iter()
            .find(|episode| &episode.publisher_guid == guid)
    {
        return Some(value);
    }
    let title = page.episode_title.as_deref()?;
    let wanted = comparable_title(title);
    let mut matches = episodes
        .iter()
        .filter(|episode| comparable_title(&episode.title) == wanted);
    let first = matches.next()?;
    let second = matches.next();
    if second.is_none() || page.published_at_milliseconds.is_none() {
        return (second.is_none()).then_some(first);
    }
    episodes
        .iter()
        .filter(|episode| comparable_title(&episode.title) == wanted)
        .min_by_key(|episode| {
            episode
                .published_at
                .value
                .abs_diff(page.published_at_milliseconds.unwrap_or_default())
        })
}

fn stable_podcast_id(value: &str) -> PodcastId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/shared-podcast/v1\0");
    hash.update(value.to_lowercase());
    let value: [u8; 32] = hash.finalize().into();
    PodcastId::from_bytes(value[..16].try_into().expect("fixed digest"))
}
fn comparable_url(value: &str) -> String {
    value
        .split('#')
        .next()
        .unwrap_or(value)
        .trim_end_matches('/')
        .to_lowercase()
}
fn comparable_title(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn without_fragment(url: &Url) -> Url {
    let mut value = url.clone();
    value.set_fragment(None);
    value
}
fn looks_like_audio(url: &Url) -> bool {
    matches!(
        url.path()
            .rsplit('.')
            .next()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp3" | "m4a" | "aac" | "wav" | "ogg" | "opus")
    )
}
fn mime_type(url: &Url) -> Option<String> {
    match url.path().rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "mp3" => Some("audio/mpeg".into()),
        "m4a" => Some("audio/mp4".into()),
        "aac" => Some("audio/aac".into()),
        "wav" => Some("audio/wav".into()),
        "ogg" | "opus" => Some("audio/ogg".into()),
        _ => None,
    }
}
fn fallback_title(url: &Url) -> String {
    url.path_segments()
        .and_then(Iterator::last)
        .filter(|v| !v.is_empty())
        .map(|v| v.replace(['-', '_'], " "))
        .unwrap_or_else(|| "Shared Episode".into())
}
