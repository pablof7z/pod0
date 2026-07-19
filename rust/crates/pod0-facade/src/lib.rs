#![forbid(unsafe_code)]

use std::sync::Arc;

pub use pod0_application::{
    ApplicationCommand, CommandEnvelope, CoreFailure, CoreFailureCode, DomainEvent,
    DomainEventEnvelope, EpisodeSummary, EvidenceIndexProjection, EvidenceIndexSpanProjection,
    EvidenceIndexStage, FACADE_CONTRACT_VERSION, HostFailureCode, HostObservation,
    HostObservationEnvelope, HostRequest, HostRequestEnvelope, KernelProbeCommand,
    KernelProbeProjection, LibraryProjection, MAX_EVIDENCE_INDEX_PAGE_ITEMS,
    MAX_FEED_RESPONSE_BYTES, MAX_HOST_REQUEST_BATCH, MAX_OPERATION_ITEMS,
    MAX_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS, MAX_PROJECTION_ITEMS, MAX_RECALL_CANDIDATES,
    MAX_RECALL_EMBEDDING_DIMENSIONS, MAX_RECALL_EVIDENCE, MAX_RECALL_EXCERPT_BYTES,
    MAX_RECALL_QUERY_BYTES, MIN_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS, NativeTimerMode,
    NoteProjectionScope, NotesProjection, OperationProjection, OperationResult, OperationStage,
    PlaybackAllowedActions, PlaybackAudioRoute, PlaybackCommand, PlaybackHostState,
    PlaybackInterruption, PlaybackItem, PlaybackLifecycleObservation, PlaybackPolicyState,
    PlaybackProjection, PlaybackStopReason, PlaybackTransitionCue, PodcastSummary, Projection,
    ProjectionEnvelope, ProjectionRequest, ProjectionScope, QueuePlacement,
    RecallCandidateObservation, RecallEmbeddingVector, RecallEvidenceProjection, RecallPhase,
    RecallQuery, RecallRerankDocument, RecallRerankObservation, RecallResultProjection,
    RecallScope, RecallScoreProjection, RecallStage, Retryability, TranscriptEvidenceInput,
    TranscriptSegmentInput, UnsupportedProjection, UserAction, bounded_host_request_count,
    bounded_playback_observation_interval,
};
use pod0_application::{Clock, KernelApplication};
pub use pod0_domain::{
    ArtifactReference, AutoDownloadMode, AutoDownloadPolicy, CancellationId, CommandId,
    CompletionCause, CompletionStatus, ContentDigest, DomainEventId, DownloadArtifactStatus,
    EpisodeId, EpisodeIdentityRecord, EpisodeIdentityResolution, EpisodeListeningState,
    EpisodeRecord, EvidenceChunkPolicy, EvidenceGenerationId, EvidenceSpanId, FeedIdentityV1,
    HostRequestId, ListeningDomainError, ListeningDomainSnapshot, ListeningPlaybackPolicy,
    NoteAuthor, NoteEvidenceReference, NoteId, NoteKind, NoteRecord, NoteRevision, NoteTarget,
    PlaybackRatePermille, PlaybackSegment, PlaybackSleepMode, PodcastId, PodcastIdentityRecord,
    PodcastIdentityResolution, PodcastKind, PodcastRecord, PodcastSubscriptionRecord, QueueEntry,
    QueueEntryId, RecallQueryId, SpeakerId, StateRevision, SubscriptionId,
    TranscriptArtifactStatus, TranscriptProvenance, TranscriptSegmentId, TranscriptSource,
    TranscriptVersionId, UnixTimestampMilliseconds, make_feed_identity_v1,
    resolve_episode_identity_v1, resolve_legacy_parent_id, resolve_podcast_identity_v1,
    validate_listening_snapshot,
};

uniffi::setup_scaffolding!();

mod listening_migration;
mod note_migration;
mod runtime;
mod runtime_clock;
mod runtime_command_fingerprint;
mod runtime_command_fingerprint_values;
mod runtime_commands;
mod runtime_evidence_commands;
mod runtime_evidence_projection;
mod runtime_evidence_state;
#[cfg(test)]
mod runtime_evidence_tests;
mod runtime_feed_commands;
mod runtime_note_commands;
#[cfg(test)]
mod runtime_note_tests;
mod runtime_observations;
mod runtime_playback_actions;
mod runtime_playback_commands;
mod runtime_playback_host;
mod runtime_playback_observations;
#[cfg(test)]
mod runtime_playback_race_tests;
#[cfg(test)]
mod runtime_playback_recovery_tests;
#[cfg(test)]
mod runtime_playback_test_support;
#[cfg(test)]
mod runtime_playback_tests;
mod runtime_playback_transitions;
mod runtime_projection;
mod runtime_recall_commands;
mod runtime_recall_observations;
mod runtime_recall_rerank;
mod runtime_recall_state;
#[cfg(test)]
mod runtime_recall_test_support;
#[cfg(test)]
mod runtime_recall_tests;
mod runtime_state;
#[cfg(test)]
mod runtime_tests;
pub use listening_migration::{
    LegacyListeningBackupEvidence, LegacyListeningImportPlan, LegacyListeningImportReport,
    LegacyListeningImportVerification, LegacyListeningMigrationError, LegacyListeningSourceKind,
    SharedListeningStorePreparation, commit_staged_legacy_listening_import,
    inspect_legacy_listening_source, prepare_shared_listening_store,
    read_staged_legacy_listening_import, stage_legacy_listening_import,
};
pub use note_migration::{
    LegacyNoteBackupEvidence, LegacyNoteImportPlan, LegacyNoteImportReport,
    LegacyNoteImportVerification, LegacyNoteMigrationError, commit_staged_legacy_note_import,
    inspect_legacy_note_source, read_staged_legacy_note_import, stage_legacy_note_import,
};
pub use runtime::{FacadeOpenError, Pod0Facade};

/// Event-driven projection delivery. The generated Swift and Kotlin callback
/// interfaces derive from this single app-owned surface.
#[uniffi::export(with_foreign)]
pub trait ProjectionSubscriber: Send + Sync {
    fn receive(&self, projection: ProjectionEnvelope);
}

/// Shape of the one native/core API. Dispatch and host observation methods do
/// not return per-operation success; durable outcomes appear in projections.
pub trait Pod0ApplicationApi: Send + Sync {
    fn dispatch(&self, command: CommandEnvelope);
    fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope;
    fn subscribe(
        &self,
        request: ProjectionRequest,
        subscriber: Arc<dyn ProjectionSubscriber>,
    ) -> SubscriptionId;
    fn unsubscribe(&self, subscription_id: SubscriptionId);
    fn next_host_requests(&self, maximum_count: u16) -> Vec<HostRequestEnvelope>;
    fn record_host_observation(&self, observation: HostObservationEnvelope);
}

/// An internal deterministic probe retained for injected-time characterization.
pub struct KernelProbeFacade<C> {
    application: KernelApplication<C>,
}

impl<C: Clock> KernelProbeFacade<C> {
    #[must_use]
    pub const fn new(clock: C) -> Self {
        Self {
            application: KernelApplication::new(clock),
        }
    }

    #[must_use]
    pub fn dispatch_probe(&self, command: KernelProbeCommand) -> KernelProbeProjection {
        self.application.dispatch_probe(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> UnixTimestampMilliseconds {
            UnixTimestampMilliseconds::new(42)
        }
    }

    #[test]
    fn facade_preserves_the_typed_application_projection() {
        let command = KernelProbeCommand {
            command_id: CommandId::from_bytes([4; 16]),
        };

        let projection = KernelProbeFacade::new(FixedClock).dispatch_probe(command);

        assert_eq!(projection.command_id, command.command_id);
        assert_eq!(projection.observed_at.value(), 42);
    }

    #[test]
    fn listening_actions_are_typed_without_dynamic_dispatch() {
        let command = CommandEnvelope {
            command_id: CommandId::from_parts(0, 1),
            cancellation_id: CancellationId::from_parts(0, 2),
            expected_revision: Some(StateRevision::new(3)),
            command: ApplicationCommand::RequestPlayback {
                episode_id: EpisodeId::from_parts(0, 4),
            },
        };

        assert!(matches!(
            command.command,
            ApplicationCommand::RequestPlayback { episode_id }
                if episode_id == EpisodeId::from_parts(0, 4)
        ));
        assert_eq!(bounded_host_request_count(0), 1);
        assert_eq!(
            bounded_host_request_count(u16::MAX),
            usize::from(MAX_HOST_REQUEST_BATCH)
        );
        assert_eq!(bounded_playback_observation_interval(0), 500);
        assert_eq!(bounded_playback_observation_interval(1_000), 1_000);
        assert_eq!(bounded_playback_observation_interval(u32::MAX), 5_000);
    }
}
