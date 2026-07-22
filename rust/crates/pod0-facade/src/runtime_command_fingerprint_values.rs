use pod0_application::TranscriptEvidenceInput;
use pod0_domain::{AutoDownloadMode, AutoDownloadPolicy, TranscriptSource};
use sha2::{Digest, Sha256};

use pod0_application::ApplicationCommand;

pub(super) fn finish_command_hash(hash: Sha256) -> String {
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn hash_command_tail(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::CancelOperation { cancellation_id } => {
            hash.update(b"cancel\0");
            hash.update(cancellation_id.into_bytes());
        }
        ApplicationCommand::Unsupported { wire_code } => {
            hash.update(b"unsupported\0");
            hash.update(wire_code.to_be_bytes());
        }
        _ => unreachable!("tail fingerprint called for another command"),
    }
}

pub(super) fn hash_note_kind(hash: &mut Sha256, value: pod0_domain::NoteKind) {
    match value {
        pod0_domain::NoteKind::Free => hash.update([1]),
        pod0_domain::NoteKind::Reflection => hash.update([2]),
        pod0_domain::NoteKind::SystemEvent => hash.update([3]),
        pod0_domain::NoteKind::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

pub(super) fn hash_note_author(hash: &mut Sha256, value: pod0_domain::NoteAuthor) {
    match value {
        pod0_domain::NoteAuthor::User => hash.update([1]),
        pod0_domain::NoteAuthor::Agent => hash.update([2]),
        pod0_domain::NoteAuthor::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

pub(super) fn hash_note_target(hash: &mut Sha256, value: Option<pod0_domain::NoteTarget>) {
    match value {
        None => hash.update([0]),
        Some(pod0_domain::NoteTarget::Note { note_id }) => {
            hash.update([1]);
            hash.update(note_id.into_bytes());
        }
        Some(pod0_domain::NoteTarget::Episode {
            episode_id,
            position_milliseconds,
        }) => {
            hash.update([2]);
            hash.update(episode_id.into_bytes());
            hash.update(position_milliseconds.to_be_bytes());
        }
        Some(pod0_domain::NoteTarget::Unsupported { wire_code }) => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

pub(super) fn hash_optional_speaker(hash: &mut Sha256, value: Option<pod0_domain::SpeakerId>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash.update(value.into_bytes());
        }
        None => hash.update([0]),
    }
}

pub(super) fn hash_clip_source(hash: &mut Sha256, value: pod0_domain::ClipSource) {
    match value {
        pod0_domain::ClipSource::Touch => hash.update([1]),
        pod0_domain::ClipSource::Auto => hash.update([2]),
        pod0_domain::ClipSource::Headphone => hash.update([3]),
        pod0_domain::ClipSource::Carplay => hash.update([4]),
        pod0_domain::ClipSource::Watch => hash.update([5]),
        pod0_domain::ClipSource::Siri => hash.update([6]),
        pod0_domain::ClipSource::Agent => hash.update([7]),
        pod0_domain::ClipSource::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

pub(super) fn hash_optional(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash.update(value.as_bytes());
        }
        None => hash.update([0]),
    }
    hash.update([0]);
}

pub(super) fn hash_policy(hash: &mut Sha256, policy: &AutoDownloadPolicy) {
    match policy.mode {
        AutoDownloadMode::Off => hash.update([1]),
        AutoDownloadMode::Latest { count } => {
            hash.update([2]);
            hash.update(count.to_be_bytes());
        }
        AutoDownloadMode::AllNew => hash.update([3]),
        AutoDownloadMode::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.update([u8::from(policy.wifi_only)]);
}

pub(super) fn hash_evidence_input(
    hash: &mut Sha256,
    input: &TranscriptEvidenceInput,
    policy: pod0_domain::EvidenceChunkPolicy,
) {
    hash.update(b"rebuild-evidence\0");
    hash.update(input.episode_id.into_bytes());
    hash.update(input.podcast_id.into_bytes());
    hash.update(input.source_revision.as_bytes());
    hash.update([0]);
    hash.update(
        match input.source {
            TranscriptSource::Publisher => 1_u32,
            TranscriptSource::Scribe => 2,
            TranscriptSource::Whisper => 3,
            TranscriptSource::OnDevice => 4,
            TranscriptSource::AssemblyAi => 5,
            TranscriptSource::Other => 6,
            TranscriptSource::Unsupported { wire_code } => wire_code | 0x8000_0000,
        }
        .to_be_bytes(),
    );
    hash_optional(hash, input.provider.as_deref());
    hash.update(input.source_payload_digest.into_bytes());
    hash.update(policy.version.to_be_bytes());
    hash.update(policy.target_tokens.to_be_bytes());
    hash.update(policy.overlap_per_mille.to_be_bytes());
    hash.update(policy.snap_tolerance_per_mille.to_be_bytes());
    hash.update((input.segments.len() as u64).to_be_bytes());
    for segment in &input.segments {
        hash.update(segment.text.as_bytes());
        hash.update([0]);
        hash.update(segment.start_milliseconds.to_be_bytes());
        hash.update(segment.end_milliseconds.to_be_bytes());
        match segment.speaker_id {
            Some(id) => {
                hash.update([1]);
                hash.update(id.into_bytes());
            }
            None => hash.update([0]),
        }
    }
}
