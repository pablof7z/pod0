use regex::Regex;
use serde_json::Value;
use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};
use url::Url;

use crate::EpisodeWebPageMetadata;

#[must_use]
pub fn parse_episode_web_page(bytes: &[u8], base_url: &str) -> Option<EpisodeWebPageMetadata> {
    let base = Url::parse(base_url).ok()?;
    if !matches!(base.scheme(), "http" | "https") {
        return None;
    }
    let html = String::from_utf8_lossy(bytes);
    let meta = crate::episode_web_metadata_html::metadata(&html);
    let links = crate::episode_web_metadata_html::links(&html);
    let json = crate::episode_web_metadata_html::podcast_episode_json(&html);
    let episode_id = base
        .query_pairs()
        .find_map(|(key, value)| (key == "i").then(|| value.into_owned()));
    let marker = episode_id
        .as_ref()
        .map(|id| format!(r#""contentId":"{id}""#));

    let mut episode_title = clean(
        json_string(json.as_ref(), &["name"])
            .or_else(|| meta.get("og:title").cloned())
            .or_else(|| meta.get("twitter:title").cloned())
            .or_else(|| meta.get("apple:title").cloned()),
    );
    let mut podcast_title = clean(
        json_string(json.as_ref(), &["partOfSeries", "name"])
            .or_else(|| json_string(json.as_ref(), &["productionCompany"])),
    );
    let linked_feed = links.iter().find(|link| {
        link.rel.contains("alternate")
            && link.content_type.as_ref().is_some_and(|value| {
                contains_case_insensitive(value, "rss") || contains_case_insensitive(value, "atom")
            })
    });
    let embedded_feed =
        crate::episode_web_metadata_html::json_string_field("feedUrl", &html, marker.as_deref());
    let feed_url = resolve_url(
        linked_feed
            .map(|link| link.href.as_str())
            .or(embedded_feed.as_deref()),
        &base,
    );
    let audio_url = resolve_url(
        meta.get("twitter:player:stream")
            .or_else(|| meta.get("og:audio"))
            .or_else(|| meta.get("og:audio:url"))
            .cloned()
            .or_else(|| crate::episode_web_metadata_html::first_media_url(json.as_ref()))
            .or_else(|| {
                crate::episode_web_metadata_html::json_string_field(
                    "streamUrl",
                    &html,
                    marker.as_deref(),
                )
            })
            .or_else(|| crate::episode_web_metadata_html::first_audio_source(&html))
            .as_deref(),
        &base,
    );

    if base
        .host_str()
        .is_some_and(|host| contains_case_insensitive(host, "overcast.fm"))
        && podcast_title.is_none()
        && let Some(combined) = episode_title.as_deref()
        && let Some((episode, podcast)) = split_overcast_title(combined)
    {
        episode_title = Some(episode);
        podcast_title = Some(podcast);
    }

    Some(EpisodeWebPageMetadata {
        episode_title,
        podcast_title,
        description: clean(
            json_string(json.as_ref(), &["description"])
                .or_else(|| meta.get("apple:description").cloned())
                .or_else(|| meta.get("og:description").cloned())
                .or_else(|| meta.get("twitter:description").cloned()),
        ),
        published_at_milliseconds: json_string(json.as_ref(), &["datePublished"])
            .as_deref()
            .and_then(parse_date_milliseconds),
        duration_milliseconds: json_string(json.as_ref(), &["duration"])
            .as_deref()
            .and_then(parse_duration_milliseconds),
        audio_url,
        audio_mime_type: meta
            .get("twitter:player:stream:content_type")
            .or_else(|| meta.get("og:audio:type"))
            .cloned(),
        image_url: resolve_url(
            json_string(json.as_ref(), &["thumbnailUrl"])
                .or_else(|| meta.get("og:image").cloned())
                .or_else(|| meta.get("twitter:image").cloned())
                .as_deref(),
            &base,
        ),
        feed_url,
        canonical_url: resolve_url(
            links
                .iter()
                .find(|link| link.rel == "canonical")
                .map(|link| link.href.as_str())
                .or_else(|| meta.get("og:url").map(String::as_str)),
            &base,
        ),
        apple_podcast_id: apple_podcast_id(base.as_str()).or_else(|| apple_podcast_id(&html)),
        guid: crate::episode_web_metadata_html::json_string_field("guid", &html, marker.as_deref()),
    })
}

fn clean(value: Option<String>) -> Option<String> {
    let value = crate::episode_web_metadata_entities::decode_html_entities(value?.trim());
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

fn resolve_url(raw: Option<&str>, base: &Url) -> Option<String> {
    let raw = clean(raw.map(str::to_owned))?;
    let value = if raw.starts_with("//") {
        Url::parse(&format!("{}:{raw}", base.scheme())).ok()?
    } else {
        base.join(&raw).ok()?
    };
    matches!(value.scheme(), "http" | "https").then(|| value.into())
}

fn json_string(value: Option<&Value>, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value?, |current, key| current.get(key))?
        .as_str()
        .map(str::to_owned)
}

fn parse_date_milliseconds(value: &str) -> Option<i64> {
    if let Ok(value) = OffsetDateTime::parse(value, &Rfc3339) {
        return i64::try_from(value.unix_timestamp_nanos() / 1_000_000).ok();
    }
    let format = time::format_description::parse("[year]-[month]-[day]").ok()?;
    let date = Date::parse(value, &format).ok()?;
    Some(date.midnight().assume_utc().unix_timestamp() * 1_000)
}

fn parse_duration_milliseconds(value: &str) -> Option<u64> {
    let pattern =
        Regex::new(r"^PT(?:(\d+(?:\.\d+)?)H)?(?:(\d+(?:\.\d+)?)M)?(?:(\d+(?:\.\d+)?)S)?$").ok()?;
    let captures = pattern.captures(value)?;
    let number = |index| {
        captures
            .get(index)
            .and_then(|value| value.as_str().parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let milliseconds = (number(1) * 3_600_000.0) + (number(2) * 60_000.0) + (number(3) * 1_000.0);
    milliseconds
        .is_finite()
        .then(|| milliseconds.round() as u64)
}

fn apple_podcast_id(value: &str) -> Option<String> {
    let pattern = Regex::new(r#"(?i)podcasts\.apple\.com[^\s\"'<>]*/id(\d{5,})"#).ok()?;
    pattern
        .captures(value)?
        .get(1)
        .map(|value| value.as_str().to_owned())
}

fn split_overcast_title(value: &str) -> Option<(String, String)> {
    let (episode, podcast) = value.rsplit_once(" — ")?;
    Some((clean(Some(episode.into()))?, clean(Some(podcast.into()))?))
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}
