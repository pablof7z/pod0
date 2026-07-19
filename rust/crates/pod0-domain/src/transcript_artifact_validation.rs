use std::collections::BTreeSet;

use crate::{
    CanonicalTranscriptSegment, MAX_TRANSCRIPT_LANGUAGE_BYTES, MAX_TRANSCRIPT_SPEAKER_LABEL_BYTES,
    MAX_TRANSCRIPT_SPEAKERS, MAX_TRANSCRIPT_WORD_TEXT_BYTES, MAX_TRANSCRIPT_WORDS,
    TranscriptArtifactError, TranscriptArtifactInput, TranscriptArtifactWordInput,
};

pub(crate) fn validate_metadata(
    input: &TranscriptArtifactInput,
) -> Result<(), TranscriptArtifactError> {
    if input.source_revision.is_empty()
        || input.source_revision.trim() != input.source_revision
        || input.source_revision.len() > crate::MAX_SOURCE_REVISION_BYTES
    {
        return Err(TranscriptArtifactError::InvalidSourceRevision);
    }
    if input.provider.as_ref().is_some_and(|value| {
        value.is_empty()
            || value.trim() != value
            || value.len() > crate::MAX_PROVENANCE_PROVIDER_BYTES
    }) {
        return Err(TranscriptArtifactError::InvalidProvider);
    }
    if !valid_language(&input.language) {
        return Err(TranscriptArtifactError::InvalidLanguage);
    }
    if input.speakers.len() > MAX_TRANSCRIPT_SPEAKERS {
        return Err(TranscriptArtifactError::TooManySpeakers);
    }
    let mut ids = BTreeSet::new();
    for speaker in &input.speakers {
        if !ids.insert(speaker.speaker_id) {
            return Err(TranscriptArtifactError::DuplicateSpeaker);
        }
        if invalid_bounded_text(&speaker.label, MAX_TRANSCRIPT_SPEAKER_LABEL_BYTES)
            || speaker.display_name.as_ref().is_some_and(|value| {
                invalid_bounded_text(value, MAX_TRANSCRIPT_SPEAKER_LABEL_BYTES)
            })
        {
            return Err(TranscriptArtifactError::InvalidSpeakerLabel);
        }
    }
    Ok(())
}

pub(crate) fn canonical_segments(
    input: &TranscriptArtifactInput,
) -> Result<Vec<CanonicalTranscriptSegment>, TranscriptArtifactError> {
    if input.segments.len() > crate::MAX_TRANSCRIPT_SEGMENTS {
        return Err(TranscriptArtifactError::TooManySegments);
    }
    let mut canonical = Vec::with_capacity(input.segments.len());
    let mut previous_start = None;
    let mut total_bytes = 0_usize;
    let mut total_words = 0_usize;
    for (index, segment) in input.segments.iter().enumerate() {
        let ordinal = u32::try_from(index).map_err(|_| TranscriptArtifactError::TooManySegments)?;
        if segment.text.trim().is_empty() {
            return Err(TranscriptArtifactError::InvalidSegmentText);
        }
        if segment.text.len() > crate::MAX_SEGMENT_TEXT_BYTES {
            return Err(TranscriptArtifactError::SegmentTextTooLong);
        }
        if segment.end_milliseconds < segment.start_milliseconds {
            return Err(TranscriptArtifactError::InvalidSegmentTime);
        }
        if previous_start.is_some_and(|start| segment.start_milliseconds < start) {
            return Err(TranscriptArtifactError::SegmentsOutOfOrder);
        }
        previous_start = Some(segment.start_milliseconds);
        validate_words(&segment.words)?;
        total_words = total_words.saturating_add(segment.words.len());
        if total_words > MAX_TRANSCRIPT_WORDS {
            return Err(TranscriptArtifactError::TooManyWords);
        }
        total_bytes = total_bytes
            .saturating_add(segment.text.len())
            .saturating_add(
                segment
                    .words
                    .iter()
                    .map(|word| word.text.len())
                    .sum::<usize>(),
            );
        if total_bytes > crate::MAX_TRANSCRIPT_BYTES {
            return Err(TranscriptArtifactError::TranscriptTooLarge);
        }
        canonical.push(CanonicalTranscriptSegment {
            ordinal,
            text: normalized_text(&segment.text),
            start_milliseconds: segment.start_milliseconds,
            end_milliseconds: segment.end_milliseconds,
            speaker_id: segment.speaker_id,
        });
    }
    Ok(canonical)
}

fn validate_words(words: &[TranscriptArtifactWordInput]) -> Result<(), TranscriptArtifactError> {
    let mut previous_start = None;
    for word in words {
        if invalid_bounded_text(&word.text, MAX_TRANSCRIPT_WORD_TEXT_BYTES) {
            return Err(TranscriptArtifactError::InvalidWordText);
        }
        if word.end_milliseconds < word.start_milliseconds {
            return Err(TranscriptArtifactError::InvalidWordTime);
        }
        if previous_start.is_some_and(|start| word.start_milliseconds < start) {
            return Err(TranscriptArtifactError::WordsOutOfOrder);
        }
        previous_start = Some(word.start_milliseconds);
    }
    Ok(())
}

fn valid_language(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TRANSCRIPT_LANGUAGE_BYTES
        && value.split('-').all(|part| {
            !part.is_empty()
                && part.len() <= 8
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn invalid_bounded_text(value: &str, limit: usize) -> bool {
    value.trim().is_empty() || value.len() > limit
}

fn normalized_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
