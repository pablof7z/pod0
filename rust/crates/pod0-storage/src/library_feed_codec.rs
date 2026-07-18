use pod0_domain::{
    EpisodeFeedMetadata, PodcastPersonRecord, PodcastSoundBiteRecord, PublisherTranscriptFormat,
    PublisherTranscriptReference,
};
use serde::{Deserialize, Serialize};

use crate::StorageError;

#[derive(Serialize, Deserialize)]
struct StoredPerson {
    name: String,
    role: Option<String>,
    group: Option<String>,
    image_url: Option<String>,
    link_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredSoundBite {
    start_milliseconds: u64,
    duration_milliseconds: u64,
    title: Option<String>,
}

pub(crate) struct EncodedFeedMetadata {
    pub transcript_url: Option<String>,
    pub transcript_media_type: Option<String>,
    pub transcript_format_code: Option<i64>,
    pub transcript_format_wire_code: Option<i64>,
    pub chapters_url: Option<String>,
    pub persons_json: String,
    pub sound_bites_json: String,
}

pub(crate) fn encode(metadata: &EpisodeFeedMetadata) -> Result<EncodedFeedMetadata, StorageError> {
    let persons = metadata
        .persons
        .iter()
        .map(|person| StoredPerson {
            name: person.name.clone(),
            role: person.role.clone(),
            group: person.group.clone(),
            image_url: person.image_url.clone(),
            link_url: person.link_url.clone(),
        })
        .collect::<Vec<_>>();
    let sound_bites = metadata
        .sound_bites
        .iter()
        .map(|sound_bite| StoredSoundBite {
            start_milliseconds: sound_bite.start_milliseconds,
            duration_milliseconds: sound_bite.duration_milliseconds,
            title: sound_bite.title.clone(),
        })
        .collect::<Vec<_>>();
    let (transcript_url, transcript_media_type, transcript_format_code, transcript_wire) = metadata
        .publisher_transcript
        .as_ref()
        .map_or((None, None, None, None), |transcript| {
            let (code, wire) = transcript_format(&transcript.format);
            (
                Some(transcript.url.clone()),
                transcript.media_type.clone(),
                Some(code),
                wire,
            )
        });
    Ok(EncodedFeedMetadata {
        transcript_url,
        transcript_media_type,
        transcript_format_code,
        transcript_format_wire_code: transcript_wire,
        chapters_url: metadata.chapters_url.clone(),
        persons_json: serde_json::to_string(&persons).map_err(|_| corrupt())?,
        sound_bites_json: serde_json::to_string(&sound_bites).map_err(|_| corrupt())?,
    })
}

pub(crate) fn decode(
    transcript_url: Option<String>,
    transcript_media_type: Option<String>,
    transcript_format_code: Option<i64>,
    transcript_format_wire: Option<i64>,
    chapters_url: Option<String>,
    persons_json: Option<String>,
    sound_bites_json: Option<String>,
) -> Result<EpisodeFeedMetadata, StorageError> {
    let publisher_transcript = match (transcript_url, transcript_format_code) {
        (Some(url), Some(code)) => Some(PublisherTranscriptReference {
            url,
            media_type: transcript_media_type,
            format: decode_transcript_format(code, transcript_format_wire)?,
        }),
        (None, None) => None,
        _ => return Err(corrupt()),
    };
    let persons: Vec<StoredPerson> =
        serde_json::from_str(persons_json.as_deref().unwrap_or("[]")).map_err(|_| corrupt())?;
    let sound_bites: Vec<StoredSoundBite> =
        serde_json::from_str(sound_bites_json.as_deref().unwrap_or("[]")).map_err(|_| corrupt())?;
    Ok(EpisodeFeedMetadata {
        publisher_transcript,
        chapters_url,
        persons: persons
            .into_iter()
            .map(|person| PodcastPersonRecord {
                name: person.name,
                role: person.role,
                group: person.group,
                image_url: person.image_url,
                link_url: person.link_url,
            })
            .collect(),
        sound_bites: sound_bites
            .into_iter()
            .map(|sound_bite| PodcastSoundBiteRecord {
                start_milliseconds: sound_bite.start_milliseconds,
                duration_milliseconds: sound_bite.duration_milliseconds,
                title: sound_bite.title,
            })
            .collect(),
    })
}

fn transcript_format(value: &PublisherTranscriptFormat) -> (i64, Option<i64>) {
    match value {
        PublisherTranscriptFormat::Json => (1, None),
        PublisherTranscriptFormat::WebVtt => (2, None),
        PublisherTranscriptFormat::SubRip => (3, None),
        PublisherTranscriptFormat::Html => (4, None),
        PublisherTranscriptFormat::PlainText => (5, None),
        PublisherTranscriptFormat::Unknown => (6, None),
        PublisherTranscriptFormat::Unsupported { wire_code } => (255, Some(i64::from(*wire_code))),
    }
}

fn decode_transcript_format(
    code: i64,
    wire: Option<i64>,
) -> Result<PublisherTranscriptFormat, StorageError> {
    Ok(match code {
        1 => PublisherTranscriptFormat::Json,
        2 => PublisherTranscriptFormat::WebVtt,
        3 => PublisherTranscriptFormat::SubRip,
        4 => PublisherTranscriptFormat::Html,
        5 => PublisherTranscriptFormat::PlainText,
        6 => PublisherTranscriptFormat::Unknown,
        255 => PublisherTranscriptFormat::Unsupported {
            wire_code: u32::try_from(wire.ok_or_else(corrupt)?).map_err(|_| corrupt())?,
        },
        _ => return Err(corrupt()),
    })
}

fn corrupt() -> StorageError {
    StorageError::CorruptSchema {
        detail: "episode feed metadata is malformed",
    }
}
