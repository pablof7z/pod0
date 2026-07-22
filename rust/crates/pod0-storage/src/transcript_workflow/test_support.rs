use pod0_application::{
    TranscriptProvider, transcript_attempt_id, transcript_submission_fence_id,
    transcript_workflow_id,
};
use pod0_domain::{CancellationId, CommandId, ContentDigest, HostRequestId, StateRevision};

use super::*;
use crate::transcript_store_test_support::TranscriptFixture;
use crate::{LibraryStore, StorageError};

pub(super) const NOW: i64 = 1_800_000_100_000;

pub(super) struct Fixture {
    pub transcript: TranscriptFixture,
    pub store: LibraryStore,
    pub episode_id: pod0_domain::EpisodeId,
}

impl Fixture {
    pub fn new() -> Self {
        let transcript = TranscriptFixture::new();
        let store = LibraryStore::open_authoritative(&transcript.import.target).unwrap();
        let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
        let rows = Vec::new();
        let source_fingerprint = transcript_workflow_source_fingerprint(&rows);
        store
            .stage_legacy_transcript_workflow_cutover(LegacyTranscriptWorkflowCutoverInput {
                source_generation: 1,
                source_fingerprint,
                backup_digest: ContentDigest::from_bytes([0x91; 32]),
                backup_byte_count: 0,
                rows,
                candidates: Vec::new(),
                command_id: CommandId::from_parts(91, 1),
                cancellation_id: CancellationId::from_parts(91, 2),
                issued_revision: StateRevision::INITIAL,
                max_attempts: 8,
                now_ms: NOW,
            })
            .unwrap();
        store
            .verify_legacy_transcript_workflow_cutover(1, source_fingerprint, NOW + 1)
            .unwrap();
        store
            .commit_legacy_transcript_workflow_cutover(1, source_fingerprint, NOW + 2)
            .unwrap();
        Self {
            transcript,
            store,
            episode_id,
        }
    }

    pub fn request(&self, publisher_first: bool) -> StoredTranscriptWorkflowRequest {
        let source_revision = "audio-v1".to_owned();
        let model = "universal-3-pro".to_owned();
        StoredTranscriptWorkflowRequest {
            workflow_id: transcript_workflow_id(
                self.episode_id,
                &source_revision,
                TranscriptProvider::AssemblyAi,
                &model,
            ),
            source_revision,
            origin: "user".to_owned(),
            provider: "assembly-ai".to_owned(),
            model,
            remote_audio_url: "https://example.test/episode.mp3".to_owned(),
            local_audio_url: None,
            publisher_transcript_url: publisher_first
                .then(|| "https://example.test/transcript.vtt".to_owned()),
            publisher_mime_hint: publisher_first.then(|| "text/vtt".to_owned()),
            publisher_first,
            provider_fallback_enabled: true,
        }
    }

    pub fn attempt(&self, number: u16) -> PreparedTranscriptAttempt {
        let workflow = self.request(false).workflow_id;
        let attempt_id = transcript_attempt_id(workflow, number).unwrap();
        PreparedTranscriptAttempt {
            attempt: number,
            attempt_id,
            submission_fence_id: transcript_submission_fence_id(attempt_id),
        }
    }

    pub fn ensure_provider(&self, number: u16) -> TranscriptWorkflowRecord {
        let attempt = self.attempt(number);
        let outcome = self
            .store
            .ensure_transcript_workflow(TranscriptWorkflowEnsureInput {
                episode_id: self.episode_id,
                request: self.request(false),
                stage: StoredTranscriptWorkflowStage::Requested,
                prepared_attempt: Some(attempt),
                command_id: CommandId::from_parts(92, u64::from(number)),
                cancellation_id: CancellationId::from_parts(93, u64::from(number)),
                request_id: Some(HostRequestId::from_parts(94, u64::from(number))),
                issued_revision: StateRevision::new(u64::from(number)),
                deadline_at_ms: Some(NOW + 60_000),
                expected_selection_revision: StateRevision::INITIAL,
                max_attempts: 8,
                now_ms: NOW + i64::from(number),
                expected_workflow_revision: None,
            })
            .unwrap();
        changed(outcome)
    }

    pub fn reopen(&self) -> LibraryStore {
        LibraryStore::open_authoritative(&self.transcript.import.target).unwrap()
    }
}

pub(super) fn changed(outcome: TranscriptWorkflowEnsureOutcome) -> TranscriptWorkflowRecord {
    match outcome {
        TranscriptWorkflowEnsureOutcome::Changed(record)
        | TranscriptWorkflowEnsureOutcome::Existing(record) => record,
    }
}

pub(super) fn interrupted() -> Result<(), StorageError> {
    Err(StorageError::Interrupted)
}
