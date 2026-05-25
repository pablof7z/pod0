use std::fmt;

use chrono::Utc;
use podcast_core::{NostrVisibility, Episode, Podcast, PodcastId, PodcastKind};
use quick_xml::events::Event;
use quick_xml::Reader;
use url::Url;

use crate::rss::state::{local_name, ParserState};

/// Parsed feed result. `podcast` carries channel-level fields with `id` set
/// to the caller-provided `podcast_id` so callers can chain straight into
/// persistence. Mirrors the Swift `RSSParser.ParsedFeed`.
#[derive(Debug, Clone)]
pub struct ParsedFeed {
    pub podcast: Podcast,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MalformedXml(String),
    MissingChannel,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MalformedXml(_) => write!(
                f,
                "This URL doesn't look like a podcast feed. The server returned something other than valid RSS — double-check the URL and try again."
            ),
            ParseError::MissingChannel => write!(
                f,
                "The feed is missing its <channel> element, so there's nothing to subscribe to."
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses raw RSS bytes against a canonical `feed_url`. Streaming SAX walk
/// via `quick-xml`; mirrors the Swift `RSSParserDelegate` element-handling
/// semantics one-for-one (first-seen-wins for channel title/author/desc;
/// `content:encoded` overrides `<description>`; `itunes:summary` is a
/// fallback only).
pub fn parse_feed(
    xml: &[u8],
    feed_url: &Url,
    podcast_id: PodcastId,
) -> Result<ParsedFeed, ParseError> {
    let mut reader = Reader::from_reader(xml);
    let config = reader.config_mut();
    config.trim_text(false);
    config.expand_empty_elements = false;
    config.check_end_names = true;

    let mut state = ParserState::new(podcast_id, feed_url.clone());
    let mut buf: Vec<u8> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => state.handle_start(e)?,
            Ok(Event::Empty(ref e)) => {
                state.handle_start(e)?;
                state.handle_end(&local_name(e.name().as_ref()));
            }
            Ok(Event::End(ref e)) => state.handle_end(&local_name(e.name().as_ref())),
            Ok(Event::Text(t)) => {
                let s = t
                    .unescape()
                    .map_err(|e| ParseError::MalformedXml(e.to_string()))?;
                state.text_buffer.push_str(&s);
            }
            Ok(Event::CData(c)) => {
                let s = String::from_utf8_lossy(c.as_ref());
                state.text_buffer.push_str(&s);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ParseError::MalformedXml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    if !state.saw_channel {
        return Err(ParseError::MissingChannel);
    }

    let now = Utc::now();
    let podcast = Podcast {
        id: podcast_id,
        kind: PodcastKind::Rss,
        feed_url: Some(feed_url.clone()),
        title: state.channel_title.trim().to_string(),
        author: state.channel_author.trim().to_string(),
        image_url: state.channel_image_url,
        description: state.channel_description.trim().to_string(),
        language: nil_if_blank(&state.channel_language),
        categories: state.channel_categories,
        discovered_at: now,
        owner_pubkey_hex: None,
        nostr_visibility: NostrVisibility::Public,
        nostr_coordinate: None,
        title_is_placeholder: false,
        last_refreshed_at: Some(now),
        etag: None,
        last_modified: None,
    };

    Ok(ParsedFeed {
        podcast,
        episodes: state.episodes,
    })
}

fn nil_if_blank(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
