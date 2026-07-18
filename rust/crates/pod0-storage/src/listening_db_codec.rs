use pod0_domain::{
    AutoDownloadMode, CompletionCause, CompletionStatus, DownloadArtifactStatus, PlaybackSleepMode,
    PodcastKind, TranscriptArtifactStatus, TranscriptSource,
};

use crate::StorageError;

pub(crate) fn podcast_kind(value: &PodcastKind) -> (i64, Option<i64>) {
    match value {
        PodcastKind::Rss => (1, None),
        PodcastKind::Synthetic => (2, None),
        PodcastKind::Unsupported { wire_code } => (255, Some(i64::from(*wire_code))),
    }
}

pub(crate) fn decode_podcast_kind(
    code: i64,
    wire: Option<i64>,
) -> Result<PodcastKind, StorageError> {
    match code {
        1 => Ok(PodcastKind::Rss),
        2 => Ok(PodcastKind::Synthetic),
        255 => Ok(PodcastKind::Unsupported {
            wire_code: u32_value(wire, "podcast kind wire code")?,
        }),
        _ => Err(corrupt("podcast kind code")),
    }
}

pub(crate) fn auto_download(value: &AutoDownloadMode) -> (i64, Option<i64>, Option<i64>) {
    match value {
        AutoDownloadMode::Off => (1, None, None),
        AutoDownloadMode::Latest { count } => (2, None, Some(i64::from(*count))),
        AutoDownloadMode::AllNew => (3, None, None),
        AutoDownloadMode::Unsupported { wire_code } => (255, Some(i64::from(*wire_code)), None),
    }
}

pub(crate) fn decode_auto_download(
    code: i64,
    wire: Option<i64>,
    latest: Option<i64>,
) -> Result<AutoDownloadMode, StorageError> {
    match code {
        1 => Ok(AutoDownloadMode::Off),
        2 => Ok(AutoDownloadMode::Latest {
            count: u16::try_from(latest.ok_or_else(|| corrupt("latest count"))?)
                .map_err(|_| corrupt("latest count"))?,
        }),
        3 => Ok(AutoDownloadMode::AllNew),
        255 => Ok(AutoDownloadMode::Unsupported {
            wire_code: u32_value(wire, "auto-download wire code")?,
        }),
        _ => Err(corrupt("auto-download code")),
    }
}

pub(crate) fn completion(value: &CompletionStatus) -> (i64, Option<i64>, Option<i64>) {
    match value {
        CompletionStatus::InProgress => (1, None, None),
        CompletionStatus::Completed { cause } => {
            let (code, wire) = completion_cause(cause);
            (2, Some(code), wire)
        }
        CompletionStatus::Unsupported { wire_code } => (255, None, Some(i64::from(*wire_code))),
    }
}

fn completion_cause(value: &CompletionCause) -> (i64, Option<i64>) {
    match value {
        CompletionCause::NaturalEnd => (1, None),
        CompletionCause::ExplicitUserAction => (2, None),
        CompletionCause::LegacyPlayedFlag => (3, None),
        CompletionCause::Unsupported { wire_code } => (255, Some(i64::from(*wire_code))),
    }
}

pub(crate) fn decode_completion(
    code: i64,
    cause: Option<i64>,
    wire: Option<i64>,
) -> Result<CompletionStatus, StorageError> {
    match code {
        1 => Ok(CompletionStatus::InProgress),
        2 => Ok(CompletionStatus::Completed {
            cause: decode_completion_cause(cause, wire)?,
        }),
        255 => Ok(CompletionStatus::Unsupported {
            wire_code: u32_value(wire, "completion wire code")?,
        }),
        _ => Err(corrupt("completion code")),
    }
}

fn decode_completion_cause(
    code: Option<i64>,
    wire: Option<i64>,
) -> Result<CompletionCause, StorageError> {
    match code.ok_or_else(|| corrupt("completion cause"))? {
        1 => Ok(CompletionCause::NaturalEnd),
        2 => Ok(CompletionCause::ExplicitUserAction),
        3 => Ok(CompletionCause::LegacyPlayedFlag),
        255 => Ok(CompletionCause::Unsupported {
            wire_code: u32_value(wire, "completion cause wire code")?,
        }),
        _ => Err(corrupt("completion cause")),
    }
}

pub(crate) fn download(value: &DownloadArtifactStatus) -> (i64, Option<i64>) {
    match value {
        DownloadArtifactStatus::Unavailable => (1, None),
        DownloadArtifactStatus::Available { .. } => (2, None),
        DownloadArtifactStatus::Unsupported { wire_code } => (255, Some(i64::from(*wire_code))),
    }
}

pub(crate) fn transcript(value: &TranscriptArtifactStatus) -> (i64, Option<i64>) {
    match value {
        TranscriptArtifactStatus::Unavailable => (1, None),
        TranscriptArtifactStatus::Available { .. } => (2, None),
        TranscriptArtifactStatus::Unsupported { wire_code } => (255, Some(i64::from(*wire_code))),
    }
}

pub(crate) fn transcript_source(value: &TranscriptSource) -> (i64, Option<i64>) {
    match value {
        TranscriptSource::Publisher => (1, None),
        TranscriptSource::Scribe => (2, None),
        TranscriptSource::Whisper => (3, None),
        TranscriptSource::OnDevice => (4, None),
        TranscriptSource::AssemblyAi => (5, None),
        TranscriptSource::Other => (6, None),
        TranscriptSource::Unsupported { wire_code } => (255, Some(i64::from(*wire_code))),
    }
}

pub(crate) fn decode_transcript_source(
    code: i64,
    wire: Option<i64>,
) -> Result<TranscriptSource, StorageError> {
    match code {
        1 => Ok(TranscriptSource::Publisher),
        2 => Ok(TranscriptSource::Scribe),
        3 => Ok(TranscriptSource::Whisper),
        4 => Ok(TranscriptSource::OnDevice),
        5 => Ok(TranscriptSource::AssemblyAi),
        6 => Ok(TranscriptSource::Other),
        255 => Ok(TranscriptSource::Unsupported {
            wire_code: u32_value(wire, "transcript source wire code")?,
        }),
        _ => Err(corrupt("transcript source code")),
    }
}

pub(crate) fn sleep(
    value: &PlaybackSleepMode,
) -> Result<(i64, Option<i64>, Option<i64>), StorageError> {
    match value {
        PlaybackSleepMode::Off => Ok((1, None, None)),
        PlaybackSleepMode::Duration {
            duration_milliseconds,
        } => Ok((
            2,
            Some(i64_value(*duration_milliseconds, "sleep duration")?),
            None,
        )),
        PlaybackSleepMode::EndOfEpisode => Ok((3, None, None)),
        PlaybackSleepMode::Unsupported { wire_code } => {
            Ok((255, None, Some(i64::from(*wire_code))))
        }
    }
}

pub(crate) fn decode_sleep(
    code: i64,
    duration: Option<i64>,
    wire: Option<i64>,
) -> Result<PlaybackSleepMode, StorageError> {
    match code {
        1 => Ok(PlaybackSleepMode::Off),
        2 => Ok(PlaybackSleepMode::Duration {
            duration_milliseconds: u64::try_from(
                duration.ok_or_else(|| corrupt("sleep duration"))?,
            )
            .map_err(|_| corrupt("sleep duration"))?,
        }),
        3 => Ok(PlaybackSleepMode::EndOfEpisode),
        255 => Ok(PlaybackSleepMode::Unsupported {
            wire_code: u32_value(wire, "sleep wire code")?,
        }),
        _ => Err(corrupt("sleep mode code")),
    }
}

pub(crate) fn i64_value(value: u64, detail: &'static str) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| corrupt(detail))
}

pub(crate) const fn bool_value(value: bool) -> i64 {
    value as i64
}

pub(crate) fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}

fn u32_value(value: Option<i64>, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value.ok_or_else(|| corrupt(detail))?).map_err(|_| corrupt(detail))
}
