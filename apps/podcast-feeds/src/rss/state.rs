use podcast_core::types::transcript::TranscriptKind;
use podcast_core::{Episode, Person, PodcastId, SoundBite};
use quick_xml::events::BytesStart;
use url::Url;

use crate::rss::accumulator::{
    parse_duration, resolve_url, transcript_rank, PreferredTranscript, RssItemAccumulator,
};
use crate::rss::parser::ParseError;

/// Walks SAX events and accumulates channel + item state. The
/// `handle_start`/`handle_end` dispatch mirrors `RSSParserDelegate.swift`
/// element-by-element.
pub(crate) struct ParserState {
    pub(crate) podcast_id: PodcastId,
    feed_url: Url,

    pub(crate) saw_channel: bool,
    pub(crate) channel_title: String,
    pub(crate) channel_author: String,
    pub(crate) channel_description: String,
    pub(crate) channel_language: String,
    pub(crate) channel_image_url: Option<Url>,
    pub(crate) channel_categories: Vec<String>,

    in_item: bool,
    item: RssItemAccumulator,

    pub(crate) text_buffer: String,
    in_channel_image: bool,
    pub(crate) episodes: Vec<Episode>,
}

impl ParserState {
    pub(crate) fn new(podcast_id: PodcastId, feed_url: Url) -> Self {
        Self {
            podcast_id,
            feed_url,
            saw_channel: false,
            channel_title: String::new(),
            channel_author: String::new(),
            channel_description: String::new(),
            channel_language: String::new(),
            channel_image_url: None,
            channel_categories: Vec::new(),
            in_item: false,
            item: RssItemAccumulator::default(),
            text_buffer: String::new(),
            in_channel_image: false,
            episodes: Vec::new(),
        }
    }

    pub(crate) fn handle_start(&mut self, e: &BytesStart) -> Result<(), ParseError> {
        let name = local_name(e.name().as_ref());
        self.text_buffer.clear();

        match name.as_str() {
            "channel" => self.saw_channel = true,
            "item" => {
                self.in_item = true;
                self.item = RssItemAccumulator::default();
            }
            "image" if !self.in_item => self.in_channel_image = true,
            "enclosure" if self.in_item => self.start_enclosure(e),
            "itunes:image" => self.start_itunes_image(e),
            "itunes:category" if !self.in_item => self.start_itunes_category(e),
            "podcast:transcript" if self.in_item => self.start_transcript(e),
            "podcast:chapters" if self.in_item => self.start_chapters(e),
            "podcast:person" if self.in_item => self.start_person(e),
            "podcast:soundbite" if self.in_item => self.start_soundbite(e),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_end(&mut self, name: &str) {
        let raw = std::mem::take(&mut self.text_buffer);
        let trimmed = raw.trim().to_string();

        match name {
            "title" if !self.in_item && !self.in_channel_image && self.channel_title.is_empty() => {
                self.channel_title = trimmed;
            }
            "description" if !self.in_item && self.channel_description.is_empty() => {
                self.channel_description = trimmed;
            }
            "itunes:summary" if !self.in_item && self.channel_description.is_empty() => {
                self.channel_description = trimmed;
            }
            "language" if !self.in_item => self.channel_language = trimmed,
            "itunes:author" if !self.in_item && self.channel_author.is_empty() => {
                self.channel_author = trimmed;
            }
            "url" if self.in_channel_image && !self.in_item => {
                if let Some(url) = resolve_url(&trimmed, &self.feed_url) {
                    if self.channel_image_url.is_none() {
                        self.channel_image_url = Some(url);
                    }
                }
            }
            "image" if !self.in_item => self.in_channel_image = false,
            "title" if self.in_item => self.item.title = trimmed,
            "description" if self.in_item => {
                if self.item.description.is_empty() {
                    self.item.description = raw;
                }
            }
            "itunes:summary" if self.in_item => {
                if self.item.description.is_empty() {
                    self.item.description = raw;
                }
            }
            "content:encoded" if self.in_item => self.item.description = raw,
            "pubDate" if self.in_item => self.item.pub_date_raw = Some(trimmed),
            "guid" if self.in_item => self.item.guid = Some(trimmed),
            "itunes:duration" if self.in_item => {
                self.item.duration_secs = parse_duration(&trimmed);
            }
            "podcast:person" if self.in_item => self.end_person(trimmed),
            "podcast:soundbite" if self.in_item => self.end_soundbite(trimmed),
            "item" => {
                let item = std::mem::take(&mut self.item);
                if let Some(episode) = item.make_episode(self.podcast_id, self.feed_url.as_str()) {
                    self.episodes.push(episode);
                }
                self.in_item = false;
            }
            _ => {}
        }
    }

    fn start_enclosure(&mut self, e: &BytesStart) {
        if let Some(raw) = attribute(e, "url") {
            if let Some(url) = resolve_url(&raw, &self.feed_url) {
                self.item.enclosure_url = Some(url);
            }
        }
        self.item.enclosure_mime_type = attribute(e, "type");
    }

    fn start_itunes_image(&mut self, e: &BytesStart) {
        if let Some(href) = attribute(e, "href") {
            if let Some(url) = resolve_url(&href, &self.feed_url) {
                if self.in_item {
                    self.item.itunes_image_url = Some(url);
                } else if self.channel_image_url.is_none() {
                    self.channel_image_url = Some(url);
                }
            }
        }
    }

    fn start_itunes_category(&mut self, e: &BytesStart) {
        if let Some(text) = attribute(e, "text") {
            if !text.is_empty() && !self.channel_categories.contains(&text) {
                self.channel_categories.push(text);
            }
        }
    }

    fn start_transcript(&mut self, e: &BytesStart) {
        let Some(raw) = attribute(e, "url") else { return };
        let Some(url) = resolve_url(&raw, &self.feed_url) else { return };
        let kind = attribute(e, "type")
            .as_deref()
            .and_then(TranscriptKind::from_mime);
        let current = transcript_rank(self.item.preferred_transcript.as_ref().and_then(|p| p.kind));
        let proposed = transcript_rank(kind);
        if self.item.preferred_transcript.is_none() || proposed > current {
            self.item.preferred_transcript = Some(PreferredTranscript { url, kind });
        }
    }

    fn start_chapters(&mut self, e: &BytesStart) {
        if let Some(raw) = attribute(e, "url") {
            if let Some(url) = resolve_url(&raw, &self.feed_url) {
                self.item.chapters_url = Some(url);
            }
        }
    }

    fn start_person(&mut self, e: &BytesStart) {
        let mut person = Person::new("");
        person.role = attribute(e, "role");
        person.group = attribute(e, "group");
        person.image_url = attribute(e, "img")
            .as_deref()
            .and_then(|s| resolve_url(s, &self.feed_url));
        person.link_url = attribute(e, "href")
            .as_deref()
            .and_then(|s| resolve_url(s, &self.feed_url));
        self.item.pending_person = Some(person);
    }

    fn start_soundbite(&mut self, e: &BytesStart) {
        let start = attribute(e, "startTime").and_then(|s| s.parse::<f64>().ok());
        let dur = attribute(e, "duration").and_then(|s| s.parse::<f64>().ok());
        if let (Some(s), Some(d)) = (start, dur) {
            self.item.pending_soundbite_start = Some(s);
            self.item.pending_soundbite_duration = Some(d);
        }
    }

    fn end_person(&mut self, trimmed: String) {
        if let Some(mut person) = self.item.pending_person.take() {
            person.name = trimmed;
            if !person.name.is_empty() {
                self.item.persons.push(person);
            }
        }
    }

    fn end_soundbite(&mut self, trimmed: String) {
        if let (Some(start), Some(dur)) = (
            self.item.pending_soundbite_start,
            self.item.pending_soundbite_duration,
        ) {
            let mut bite = SoundBite::new(start, dur);
            if !trimmed.is_empty() {
                bite.title = Some(trimmed);
            }
            self.item.sound_bites.push(bite);
        }
        self.item.pending_soundbite_start = None;
        self.item.pending_soundbite_duration = None;
    }
}

pub(crate) fn local_name(qualified: &[u8]) -> String {
    // Namespace prefixes are kept (no namespace expansion mode), so the raw
    // qualified name is `prefix:local` for prefixed elements.
    String::from_utf8_lossy(qualified).to_string()
}

fn attribute(start: &BytesStart, key: &str) -> Option<String> {
    for attr in start.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == key.as_bytes() {
            return attr
                .decode_and_unescape_value(quick_xml::Reader::<&[u8]>::from_reader(&[]).decoder())
                .ok()
                .map(|c| c.into_owned());
        }
    }
    None
}
