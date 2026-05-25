use std::collections::HashSet;
use std::fmt;

use podcast_core::{NostrVisibility, Podcast, PodcastId, PodcastKind};
use quick_xml::events::Event;
use quick_xml::Reader;
use url::Url;

use chrono::Utc;

/// Parses an OPML 2.0 subscription list into seeded `Podcast` records, ready
/// for first refresh. Ports `OPMLImport.swift`.
///
/// `<outline>` entries with `xmlUrl` are kept; grouping folders without one
/// are skipped. Order is preserved as emitted. Duplicate feed URLs are
/// dropped after the first occurrence.
///
/// Return shape is `Vec<Podcast>` (not `Vec<Url>`) so the import sheet can
/// render the user's list with titles before the first refresh round-trips.
pub fn import_opml(xml: &str) -> Result<Vec<Podcast>, OpmlError> {
    let mut reader = Reader::from_str(xml);
    let config = reader.config_mut();
    config.trim_text(true);

    let mut podcasts: Vec<Podcast> = Vec::new();
    let mut seen: HashSet<Url> = HashSet::new();
    let mut buf: Vec<u8> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"outline" {
                    let mut xml_url: Option<String> = None;
                    let mut text: Option<String> = None;
                    let mut title: Option<String> = None;
                    let mut description: Option<String> = None;
                    let mut language: Option<String> = None;

                    for attr in e.attributes().with_checks(false).flatten() {
                        let key = attr.key.as_ref();
                        let value = attr
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|err| OpmlError::MalformedXml(err.to_string()))?
                            .into_owned();
                        match key {
                            b"xmlUrl" => xml_url = Some(value),
                            b"text" => text = Some(value),
                            b"title" => title = Some(value),
                            b"description" => description = Some(value),
                            b"language" => language = Some(value),
                            _ => {}
                        }
                    }

                    if let Some(url_str) = xml_url {
                        if let Ok(feed_url) = Url::parse(&url_str) {
                            if seen.insert(feed_url.clone()) {
                                let display_title = text
                                    .or(title)
                                    .unwrap_or_else(|| {
                                        feed_url
                                            .host_str()
                                            .map(String::from)
                                            .unwrap_or_else(|| feed_url.as_str().to_string())
                                    });
                                podcasts.push(seed_podcast(
                                    feed_url,
                                    display_title,
                                    description.unwrap_or_default(),
                                    language,
                                ));
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(OpmlError::MalformedXml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(podcasts)
}

fn seed_podcast(
    feed_url: Url,
    title: String,
    description: String,
    language: Option<String>,
) -> Podcast {
    Podcast {
        id: PodcastId::generate(),
        kind: PodcastKind::Rss,
        feed_url: Some(feed_url),
        title,
        author: String::new(),
        image_url: None,
        description,
        language,
        categories: Vec::new(),
        discovered_at: Utc::now(),
        owner_pubkey_hex: None,
        nostr_visibility: NostrVisibility::Public,
        nostr_coordinate: None,
        title_is_placeholder: false,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpmlError {
    MalformedXml(String),
}

impl fmt::Display for OpmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpmlError::MalformedXml(_) => write!(
                f,
                "This file isn't a valid OPML export. If you exported it from another podcast app, check that the export completed and try again."
            ),
        }
    }
}

impl std::error::Error for OpmlError {}
