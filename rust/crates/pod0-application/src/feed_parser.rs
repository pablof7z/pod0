use pod0_domain::{
    CompletionStatus, DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeListeningState,
    EpisodeRecord, PodcastId, PodcastPersonRecord, PodcastSoundBiteRecord,
    PublisherTranscriptReference, TranscriptArtifactStatus, UnixTimestampMilliseconds,
};
use quick_xml::events::BytesStart;
use url::Url;

use crate::feed::FeedParseFailure;
use crate::feed_parser_values::{
    attribute, duration, episode_id, milliseconds, name, nonempty, parse_date, transcript_format,
    transcript_rank,
};
pub(crate) struct ParsedRss {
    pub title: String,
    pub author: String,
    pub image_url: Option<String>,
    pub description: String,
    pub language: Option<String>,
    pub categories: Vec<String>,
    pub episodes: Vec<EpisodeRecord>,
}

#[derive(Default)]
struct Item {
    title: String,
    description: String,
    published_raw: Option<String>,
    guid: Option<String>,
    duration_milliseconds: Option<u64>,
    enclosure_url: Option<String>,
    enclosure_mime_type: Option<String>,
    image_url: Option<String>,
    metadata: EpisodeFeedMetadata,
    pending_person: Option<PodcastPersonRecord>,
    pending_sound_bite: Option<(u64, u64)>,
}

struct Frame {
    name: String,
    text: String,
}

pub(super) struct Parser<'a> {
    base_url: &'a Url,
    podcast_id: PodcastId,
    saw_channel: bool,
    in_item: bool,
    in_channel_image: bool,
    frames: Vec<Frame>,
    title: String,
    author: String,
    image_url: Option<String>,
    description: String,
    language: Option<String>,
    categories: Vec<String>,
    item: Item,
    episodes: Vec<EpisodeRecord>,
}

impl<'a> Parser<'a> {
    pub(super) fn new(base_url: &'a Url, podcast_id: PodcastId) -> Self {
        Self {
            base_url,
            podcast_id,
            saw_channel: false,
            in_item: false,
            in_channel_image: false,
            frames: Vec::new(),
            title: String::new(),
            author: String::new(),
            image_url: None,
            description: String::new(),
            language: None,
            categories: Vec::new(),
            item: Item::default(),
            episodes: Vec::new(),
        }
    }

    pub(super) fn start(&mut self, element: &BytesStart<'_>) {
        let element_name = name(element);
        self.frames.push(Frame {
            name: element_name.clone(),
            text: String::new(),
        });
        match element_name.as_str() {
            "channel" => self.saw_channel = true,
            "item" => {
                self.in_item = true;
                self.item = Item::default();
            }
            "image" if !self.in_item => self.in_channel_image = true,
            "enclosure" if self.in_item => {
                self.item.enclosure_url = self.url(attribute(element, b"url").as_deref());
                self.item.enclosure_mime_type = attribute(element, b"type");
            }
            "itunes:image" => {
                let resolved = self.url(attribute(element, b"href").as_deref());
                if self.in_item {
                    self.item.image_url = resolved;
                } else if self.image_url.is_none() {
                    self.image_url = resolved;
                }
            }
            "itunes:category" if !self.in_item => {
                if let Some(category) = attribute(element, b"text")
                    && !category.is_empty()
                    && !self.categories.contains(&category)
                {
                    self.categories.push(category);
                }
            }
            "podcast:transcript" if self.in_item => self.transcript(element),
            "podcast:chapters" if self.in_item => {
                self.item.metadata.chapters_url = self.url(attribute(element, b"url").as_deref());
            }
            "podcast:person" if self.in_item => {
                self.item.pending_person = Some(PodcastPersonRecord {
                    name: String::new(),
                    role: attribute(element, b"role"),
                    group: attribute(element, b"group"),
                    image_url: self.url(attribute(element, b"img").as_deref()),
                    link_url: self.url(attribute(element, b"href").as_deref()),
                });
            }
            "podcast:soundbite" if self.in_item => {
                let start = milliseconds(attribute(element, b"startTime").as_deref());
                let duration = milliseconds(attribute(element, b"duration").as_deref());
                self.item.pending_sound_bite = start.zip(duration);
            }
            _ => {}
        }
    }

    pub(super) fn text(&mut self, value: &str) {
        if let Some(frame) = self.frames.last_mut() {
            frame.text.push_str(value);
        }
    }

    pub(super) fn end(&mut self, expected: String) {
        let Some(frame) = self.frames.pop() else {
            return;
        };
        if frame.name != expected {
            return;
        }
        let trimmed = frame.text.trim();
        self.apply_text(&frame.name, &frame.text, trimmed);
    }

    fn apply_text(&mut self, element: &str, raw: &str, trimmed: &str) {
        match element {
            "title" if !self.in_item && !self.in_channel_image && self.title.is_empty() => {
                self.title = trimmed.to_owned()
            }
            "description" | "itunes:summary" if !self.in_item && self.description.is_empty() => {
                self.description = trimmed.to_owned()
            }
            "language" if !self.in_item => self.language = nonempty(trimmed),
            "itunes:author" if !self.in_item && self.author.is_empty() => {
                self.author = trimmed.to_owned()
            }
            "url" if self.in_channel_image && self.image_url.is_none() => {
                self.image_url = self.url(Some(trimmed))
            }
            "image" if !self.in_item => self.in_channel_image = false,
            "title" if self.in_item => self.item.title = trimmed.to_owned(),
            "description" | "itunes:summary"
                if self.in_item && self.item.description.is_empty() =>
            {
                self.item.description = raw.to_owned()
            }
            "content:encoded" if self.in_item => self.item.description = raw.to_owned(),
            "pubdate" if self.in_item => self.item.published_raw = nonempty(trimmed),
            "guid" if self.in_item => self.item.guid = nonempty(trimmed),
            "itunes:duration" if self.in_item => {
                self.item.duration_milliseconds = duration(trimmed)
            }
            "podcast:person" if self.in_item => self.finish_person(trimmed),
            "podcast:soundbite" if self.in_item => self.finish_sound_bite(trimmed),
            "item" => self.finish_item(),
            _ => {}
        }
    }

    fn transcript(&mut self, element: &BytesStart<'_>) {
        let Some(url) = self.url(attribute(element, b"url").as_deref()) else {
            return;
        };
        let media_type = attribute(element, b"type");
        let format = transcript_format(media_type.as_deref());
        let candidate = PublisherTranscriptReference {
            url,
            media_type,
            format,
        };
        let replace = self
            .item
            .metadata
            .publisher_transcript
            .as_ref()
            .is_none_or(|current| {
                transcript_rank(&candidate.format) > transcript_rank(&current.format)
            });
        if replace {
            self.item.metadata.publisher_transcript = Some(candidate);
        }
    }

    fn finish_person(&mut self, name: &str) {
        if let Some(mut person) = self.item.pending_person.take() {
            person.name = name.to_owned();
            if !person.name.is_empty() {
                self.item.metadata.persons.push(person);
            }
        }
    }

    fn finish_sound_bite(&mut self, title: &str) {
        if let Some((start, duration)) = self.item.pending_sound_bite.take() {
            self.item.metadata.sound_bites.push(PodcastSoundBiteRecord {
                start_milliseconds: start,
                duration_milliseconds: duration,
                title: nonempty(title),
            });
        }
    }

    fn finish_item(&mut self) {
        if let Some(enclosure_url) = self.item.enclosure_url.take() {
            let guid = self.item.guid.take().unwrap_or_else(|| {
                format!(
                    "synth::{enclosure_url}::{}",
                    self.item.published_raw.as_deref().unwrap_or("no-date")
                )
            });
            let episode_id = episode_id(self.podcast_id, &guid);
            self.episodes.push(EpisodeRecord {
                episode_id,
                podcast_id: self.podcast_id,
                publisher_guid: guid,
                title: std::mem::take(&mut self.item.title),
                description: std::mem::take(&mut self.item.description),
                published_at: UnixTimestampMilliseconds::new(parse_date(
                    self.item.published_raw.as_deref(),
                )),
                duration_milliseconds: self.item.duration_milliseconds,
                enclosure_url,
                enclosure_mime_type: self.item.enclosure_mime_type.take(),
                image_url: self.item.image_url.take(),
                feed_metadata: std::mem::take(&mut self.item.metadata),
                listening: EpisodeListeningState {
                    resume_position_milliseconds: 0,
                    completion: CompletionStatus::InProgress,
                },
                is_starred: false,
                download: DownloadArtifactStatus::Unavailable,
                transcript: TranscriptArtifactStatus::Unavailable,
                generated_audio: None,
            });
        }
        self.in_item = false;
        self.item = Item::default();
    }

    fn url(&self, raw: Option<&str>) -> Option<String> {
        let raw = raw?.trim();
        if raw.is_empty() {
            return None;
        }
        if raw.starts_with("//") {
            return Some(format!("{}:{raw}", self.base_url.scheme()));
        }
        Url::parse(raw)
            .or_else(|_| self.base_url.join(raw))
            .ok()
            .map(|url| url.to_string())
    }

    pub(super) fn finish(self) -> Result<ParsedRss, FeedParseFailure> {
        if !self.frames.is_empty() {
            return Err(FeedParseFailure::MalformedXml);
        }
        if !self.saw_channel {
            return Err(FeedParseFailure::MissingChannel);
        }
        Ok(ParsedRss {
            title: self.title,
            author: self.author,
            image_url: self.image_url,
            description: self.description,
            language: self.language,
            categories: self.categories,
            episodes: self.episodes,
        })
    }
}
