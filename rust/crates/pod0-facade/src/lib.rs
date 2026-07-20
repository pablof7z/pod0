#![forbid(unsafe_code)]

use std::sync::Arc;

pub use pod0_application::{
    AdSpanProjection, AgentComposedChapterItem, AgentComposedChapterObservation,
    ApplicationCommand, ChapterArtifactProjection, ChapterCommitReceipt, ChapterContractProjection,
    ChapterContractRejection, ChapterContractRequest, ChapterItemProjection,
    ChapterModelObservationMode, ChapterObservationLimits, ChapterObservationProjection,
    ChapterObservationRejection, ChapterPlaybackContext, ChapterProjectionScope,
    ChapterSummaryProjection, ChapterWorkflowsProjection, ClipProjectionScope, ClipsProjection,
    CommandEnvelope, CoreFailure, CoreFailureCode, DomainEvent, DomainEventEnvelope,
    EpisodeSummary, EvidenceIndexProjection, EvidenceIndexSpanProjection, EvidenceIndexStage,
    FACADE_CONTRACT_VERSION, HostCancellationRequest, HostFailureCode, HostObservation,
    HostObservationEnvelope, HostRequest, HostRequestEnvelope, KernelProbeCommand,
    KernelProbeProjection, LibraryProjection, MAX_AGENT_COMPOSED_CHAPTER_ITEMS,
    MAX_EVIDENCE_INDEX_PAGE_ITEMS, MAX_FEED_RESPONSE_BYTES, MAX_HOST_REQUEST_BATCH,
    MAX_MODEL_CHAPTER_COMPLETION_BYTES, MAX_OPERATION_ITEMS,
    MAX_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS, MAX_PROJECTION_ITEMS,
    MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES, MAX_RECALL_CANDIDATES, MAX_RECALL_EMBEDDING_BATCH,
    MAX_RECALL_EMBEDDING_DIMENSIONS, MAX_RECALL_EMBEDDING_TEXT_BYTES, MAX_RECALL_EVIDENCE,
    MAX_RECALL_EXCERPT_BYTES, MAX_RECALL_QUERY_BYTES,
    MIN_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS, ModelChapterObservation, NativeTimerMode,
    NoteProjectionScope, NotesProjection, OperationProjection, OperationResult, OperationStage,
    PlaybackAllowedActions, PlaybackAudioRoute, PlaybackCommand, PlaybackHostState,
    PlaybackInterruption, PlaybackItem, PlaybackLifecycleObservation, PlaybackPolicyState,
    PlaybackProjection, PlaybackStopReason, PlaybackTransitionCue, PodcastSummary, Projection,
    ProjectionEnvelope, ProjectionRequest, ProjectionScope, PublisherChapterObservation,
    PublisherChapterWorkflowFailure, PublisherChapterWorkflowFailureCode,
    PublisherChapterWorkflowProjection, PublisherChapterWorkflowStage, QueuePlacement,
    RecallEmbeddingInput, RecallEmbeddingVector, RecallEvidenceProjection, RecallPhase,
    RecallQuery, RecallRerankDocument, RecallRerankObservation, RecallResultProjection,
    RecallScope, RecallScoreProjection, RecallSpanEmbeddingObservation, RecallStage, Retryability,
    TranscriptCommitReceipt, TranscriptCommitRequest, TranscriptContractProjection,
    TranscriptContractRejection, TranscriptEvidenceInput, TranscriptProjection,
    TranscriptProjectionScope, TranscriptSegmentInput, TranscriptSegmentProjection,
    TranscriptSpeakerProjection, TranscriptSummaryProjection, TranscriptWordProjection,
    UnsupportedProjection, UserAction, bounded_host_request_count,
    bounded_playback_observation_interval,
};
use pod0_application::{Clock, KernelApplication};
pub use pod0_domain::{
    AdSpanEvaluation, AdSpanId, AdSpanInput, ArtifactReference, AutoDownloadMode,
    AutoDownloadPolicy, CancellationId, ChapterAdKind, ChapterArtifactId, ChapterArtifactInput,
    ChapterArtifactProvenance, ChapterArtifactSource, ChapterId, ChapterInput,
    ChapterLegacyProvenance, ChapterLegacySource, ChapterPlaybackSessionId, ClipEvidenceReference,
    ClipId, ClipRecord, ClipRevision, ClipSource, CommandId, CompletionCause, CompletionStatus,
    ContentDigest, DomainEventId, DownloadArtifactStatus, EpisodeId, EpisodeIdentityRecord,
    EpisodeIdentityResolution, EpisodeListeningState, EpisodeRecord, EvidenceChunkPolicy,
    EvidenceGenerationId, EvidenceSpanId, FeedIdentityV1, HostRequestId, ListeningDomainError,
    ListeningDomainSnapshot, ListeningPlaybackPolicy, NoteAuthor, NoteEvidenceReference, NoteId,
    NoteKind, NoteRecord, NoteRevision, NoteTarget, PlaybackRatePermille, PlaybackSeekReason,
    PlaybackSegment, PlaybackSleepMode, PodcastId, PodcastIdentityRecord,
    PodcastIdentityResolution, PodcastKind, PodcastRecord, PodcastSubscriptionRecord, QueueEntry,
    QueueEntryId, RecallQueryId, SpeakerId, StateRevision, SubscriptionId, TranscriptArtifactId,
    TranscriptArtifactInput, TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput,
    TranscriptArtifactStatus, TranscriptArtifactWordInput, TranscriptProvenance,
    TranscriptSegmentId, TranscriptSource, TranscriptVersionId, UnixTimestampMilliseconds,
    make_feed_identity_v1, resolve_episode_identity_v1, resolve_legacy_parent_id,
    resolve_podcast_identity_v1, validate_listening_snapshot,
};

uniffi::setup_scaffolding!();

mod chapter_migration;
mod chapter_migration_mapping;
#[cfg(test)]
mod chapter_migration_tests;
mod chapter_observation_facade;
mod clip_migration;
#[cfg(test)]
mod facade_contract_tests;
mod listening_migration;
mod note_migration;
mod runtime;
mod runtime_artifact_command_fingerprint;
mod runtime_cancellation;
#[cfg(test)]
mod runtime_cancellation_tests;
#[cfg(test)]
mod runtime_chapter_ad_skip_tests;
mod runtime_chapter_commands;
#[cfg(test)]
mod runtime_chapter_lifecycle_tests;
mod runtime_chapter_playback;
#[cfg(test)]
mod runtime_chapter_playback_tests;
mod runtime_chapter_projection;
#[cfg(test)]
mod runtime_chapter_tests;
mod runtime_chapter_workflow;
#[cfg(test)]
mod runtime_chapter_workflow_admission_tests;
mod runtime_chapter_workflow_commands;
mod runtime_chapter_workflow_observations;
mod runtime_chapter_workflow_projection;
#[cfg(test)]
mod runtime_chapter_workflow_race_tests;
#[cfg(test)]
mod runtime_chapter_workflow_test_support;
#[cfg(test)]
mod runtime_chapter_workflow_tests;
mod runtime_clip_commands;
#[cfg(test)]
mod runtime_clip_evidence_tests;
#[cfg(test)]
mod runtime_clip_replay_tests;
#[cfg(test)]
mod runtime_clip_tests;
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
mod runtime_feed_state;
mod runtime_note_commands;
#[cfg(test)]
mod runtime_note_evidence_tests;
#[cfg(test)]
mod runtime_note_tests;
mod runtime_observations;
mod runtime_playback_actions;
mod runtime_playback_commands;
mod runtime_playback_fingerprint;
mod runtime_playback_host;
mod runtime_playback_observations;
#[cfg(test)]
mod runtime_playback_race_tests;
#[cfg(test)]
mod runtime_playback_recovery_tests;
mod runtime_playback_state;
#[cfg(test)]
mod runtime_playback_test_support;
#[cfg(test)]
mod runtime_playback_tests;
mod runtime_playback_transitions;
mod runtime_projection;
mod runtime_recall_commands;
mod runtime_recall_cutover;
#[cfg(test)]
mod runtime_recall_cutover_tests;
mod runtime_recall_interrupts;
mod runtime_recall_observations;
mod runtime_recall_rerank;
mod runtime_recall_state;
#[cfg(test)]
mod runtime_recall_test_support;
#[cfg(test)]
mod runtime_recall_tests;
mod runtime_state;
mod runtime_storage_commands;
#[cfg(test)]
mod runtime_tests;
mod runtime_transcript_commands;
mod runtime_transcript_projection;
#[cfg(test)]
mod runtime_transcript_tests;
mod transcript_migration;
mod transcript_migration_mapping;
pub use chapter_migration::{
    LegacyChapterBackupEvidence, LegacyChapterImportPlan, LegacyChapterImportReport,
    LegacyChapterImportState, LegacyChapterImportVerification, LegacyChapterMigrationFailure,
    LegacyChapterMigrationFailureCode, LegacyChapterMigrationProjection,
    LegacyChapterMigrationStage, LegacyChapterRollbackExportReport, LegacyChapterSourceKind,
    commit_staged_legacy_chapter_import, discard_staged_legacy_chapter_import,
    export_legacy_chapter_rollback, inspect_legacy_chapter_migration,
    read_active_legacy_chapter_migration, shared_chapter_store_is_authoritative,
    stage_legacy_chapter_import, verify_staged_legacy_chapter_import,
};
pub use chapter_observation_facade::{
    chapter_observation_limits, qualify_agent_composed_chapter_observation,
    qualify_model_chapter_observation, qualify_publisher_chapter_observation,
};
pub use clip_migration::{
    LegacyClipBackupEvidence, LegacyClipImportPlan, LegacyClipImportReport,
    LegacyClipImportVerification, LegacyClipMigrationError, commit_staged_legacy_clip_import,
    inspect_legacy_clip_source, read_staged_legacy_clip_import, stage_legacy_clip_import,
};
pub use listening_migration::{
    LegacyListeningBackupEvidence, LegacyListeningImportPlan, LegacyListeningImportReport,
    LegacyListeningImportVerification, LegacyListeningMigrationError, LegacyListeningSourceKind,
    SharedListeningStorePreparation, commit_staged_legacy_listening_import,
    inspect_legacy_listening_source, prepare_shared_listening_store,
    read_staged_legacy_listening_import, shared_store_schema_version,
    stage_legacy_listening_import,
};
pub use note_migration::{
    LegacyNoteBackupEvidence, LegacyNoteImportPlan, LegacyNoteImportReport,
    LegacyNoteImportVerification, LegacyNoteMigrationError, commit_staged_legacy_note_import,
    inspect_legacy_note_source, read_staged_legacy_note_import, stage_legacy_note_import,
};
pub use runtime::{FacadeOpenError, Pod0Facade};
pub use transcript_migration::{
    LegacyTranscriptBackupEvidence, LegacyTranscriptImportPlan, LegacyTranscriptImportReport,
    LegacyTranscriptImportState, LegacyTranscriptImportVerification,
    LegacyTranscriptMigrationError, LegacyTranscriptRollbackExportReport,
    LegacyTranscriptSourceKind, commit_staged_legacy_transcript_import,
    discard_staged_legacy_transcript_import, export_legacy_transcript_rollback,
    inspect_legacy_transcript_source, read_active_legacy_transcript_import,
    shared_transcript_store_is_authoritative, stage_legacy_transcript_import,
    verify_staged_legacy_transcript_import,
};

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

    fn next_host_cancellations(&self, maximum_count: u16) -> Vec<HostCancellationRequest>;
    fn record_host_observation(&self, observation: HostObservationEnvelope);
}

/// Produces bounded, state-shaped evidence for the typed transcript contract.
/// Invalid input becomes a rejected projection rather than an exception.
/// Durable commit and selection are added by the storage slice.
#[uniffi::export]
pub fn project_transcript_contract(
    request: TranscriptCommitRequest,
    scope: TranscriptProjectionScope,
    offset: u32,
    max_items: u16,
) -> TranscriptContractProjection {
    pod0_application::project_transcript_contract(request, scope, offset, max_items)
}

/// Produces bounded, state-shaped evidence for the typed chapter contract.
/// The storage slice will add durable commit and selection after this pure
/// cross-language contract is proven.
#[uniffi::export]
pub fn project_chapter_contract(
    request: ChapterContractRequest,
    scope: ChapterProjectionScope,
    offset: u32,
    max_items: u16,
) -> ChapterContractProjection {
    pod0_application::project_chapter_contract(request, scope, offset, max_items)
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
