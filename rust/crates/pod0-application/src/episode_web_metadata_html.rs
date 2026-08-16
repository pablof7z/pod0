use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use crate::episode_web_metadata_entities::decode_html_entities;

pub(super) struct LinkTag {
    pub rel: String,
    pub content_type: Option<String>,
    pub href: String,
}

pub(super) fn metadata(html: &str) -> HashMap<String, String> {
    tag_attributes("meta", html)
        .into_iter()
        .filter_map(|attributes| {
            let key = attributes
                .get("property")
                .or_else(|| attributes.get("name"))?;
            let content = attributes.get("content")?;
            Some((key.to_ascii_lowercase(), decode_html_entities(content)))
        })
        .collect()
}

pub(super) fn links(html: &str) -> Vec<LinkTag> {
    tag_attributes("link", html)
        .into_iter()
        .filter_map(|attributes| {
            Some(LinkTag {
                rel: attributes
                    .get("rel")
                    .cloned()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                content_type: attributes.get("type").cloned(),
                href: decode_html_entities(attributes.get("href")?),
            })
        })
        .collect()
}

pub(super) fn first_audio_source(html: &str) -> Option<String> {
    ["audio", "source"]
        .into_iter()
        .flat_map(|name| tag_attributes(name, html))
        .find_map(|attributes| attributes.get("src").cloned())
        .map(|value| decode_html_entities(&value))
}

pub(super) fn podcast_episode_json(html: &str) -> Option<Value> {
    let expression = Regex::new(
        r#"(?is)<script\b[^>]*type\s*=\s*[\"']application/ld\+json[\"'][^>]*>(.*?)</script>"#,
    )
    .ok()?;
    expression.captures_iter(html).find_map(|capture| {
        let value: Value = serde_json::from_str(capture.get(1)?.as_str()).ok()?;
        find_podcast_episode(&value).cloned()
    })
}

pub(super) fn first_media_url(episode: Option<&Value>) -> Option<String> {
    ["associatedMedia", "encoding", "audio"]
        .into_iter()
        .find_map(|key| media_url(episode?.get(key)?))
}

pub(super) fn json_string_field(field: &str, html: &str, marker: Option<&str>) -> Option<String> {
    let searchable = marker
        .and_then(|marker| html.find(marker).map(|index| &html[index..]))
        .unwrap_or(html);
    let searchable = &searchable[..searchable.len().min(60_000)];
    let expression = Regex::new(&format!(
        r#""{}"\s*:\s*"((?:\\.|[^"\\])*)""#,
        regex::escape(field)
    ))
    .ok()?;
    let raw = expression.captures(searchable)?.get(1)?.as_str();
    serde_json::from_str::<String>(&format!(r#""{raw}""#)).ok()
}

fn tag_attributes(name: &str, html: &str) -> Vec<HashMap<String, String>> {
    let Ok(expression) = Regex::new(&format!(r"(?is)<{}\b[^>]*>", regex::escape(name))) else {
        return Vec::new();
    };
    expression
        .find_iter(html)
        .map(|value| attributes(value.as_str()))
        .collect()
}

fn attributes(tag: &str) -> HashMap<String, String> {
    let Ok(expression) =
        Regex::new(r#"([A-Za-z_:][A-Za-z0-9_:.\-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
    else {
        return HashMap::new();
    };
    expression
        .captures_iter(tag)
        .filter_map(|capture| {
            let key = capture.get(1)?.as_str().to_ascii_lowercase();
            let value = (2..=4)
                .find_map(|index| capture.get(index))?
                .as_str()
                .to_owned();
            Some((key, value))
        })
        .collect()
}

fn find_podcast_episode(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(values) => {
            if type_includes_episode(values.get("@type")) {
                return Some(value);
            }
            values.values().find_map(find_podcast_episode)
        }
        Value::Array(values) => values.iter().find_map(find_podcast_episode),
        _ => None,
    }
}

fn type_includes_episode(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(value)) => value == "PodcastEpisode",
        Some(Value::Array(values)) => values
            .iter()
            .any(|value| value.as_str() == Some("PodcastEpisode")),
        _ => false,
    }
}

fn media_url(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Object(values) => ["contentUrl", "embedUrl", "url"]
            .into_iter()
            .find_map(|key| values.get(key)?.as_str().map(str::to_owned)),
        Value::Array(values) => values.iter().find_map(media_url),
        _ => None,
    }
}
