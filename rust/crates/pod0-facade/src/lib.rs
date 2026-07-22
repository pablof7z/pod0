#![forbid(unsafe_code)]

use std::sync::Arc;

use pod0_application::{Clock, KernelApplication};
mod facade_exports;
pub use facade_exports::*;

uniffi::setup_scaffolding!();

mod chapter_migration;
mod chapter_migration_mapping;
#[cfg(test)]
mod chapter_migration_tests;
mod chapter_observation_facade;
mod clip_migration;
mod contract_facade;
mod download_cutover;
mod download_cutover_types;
#[cfg(test)]
mod facade_contract_tests;
mod listening_migration;
mod listening_migration_error;
mod model_chapter_cutover;
#[cfg(test)]
mod model_chapter_cutover_discard_tests;
#[cfg(test)]
mod model_chapter_cutover_success_tests;
#[cfg(test)]
mod model_chapter_cutover_tests;
mod model_chapter_cutover_types;
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
mod runtime_chapter_model_commands;
mod runtime_chapter_model_completion;
mod runtime_chapter_model_mapping;
mod runtime_chapter_model_observations;
mod runtime_chapter_model_plan;
#[cfg(test)]
mod runtime_chapter_model_plan_tests;
mod runtime_chapter_model_queue;
mod runtime_chapter_model_receipts;
#[cfg(test)]
mod runtime_chapter_model_wake_tests;
#[cfg(test)]
mod runtime_chapter_model_workflow_tests;
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
mod runtime_chapter_workflow_projection_tests;
#[cfg(test)]
mod runtime_chapter_workflow_race_tests;
#[cfg(test)]
mod runtime_chapter_workflow_test_support;
#[cfg(test)]
mod runtime_chapter_workflow_tests;
mod runtime_clip_command_fingerprint;
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
mod runtime_core_wakes;
mod runtime_download_admission;
#[cfg(test)]
mod runtime_download_admission_tests;
mod runtime_download_automatic;
mod runtime_download_command_fingerprint;
mod runtime_download_commands;
#[cfg(test)]
mod runtime_download_contract_tests;
#[cfg(test)]
mod runtime_download_deadline_tests;
mod runtime_download_mapping;
mod runtime_download_observations;
mod runtime_download_projection;
mod runtime_download_workflow;
#[cfg(test)]
mod runtime_download_workflow_tests;
mod runtime_evidence_commands;
mod runtime_evidence_completion;
mod runtime_evidence_projection;
mod runtime_evidence_state;
#[cfg(test)]
mod runtime_evidence_tests;
mod runtime_failure;
mod runtime_feed_commands;
mod runtime_feed_observations;
mod runtime_feed_state;
mod runtime_listening_commands;
mod runtime_note_commands;
#[cfg(test)]
mod runtime_note_evidence_tests;
#[cfg(test)]
mod runtime_note_tests;
mod runtime_observation_mapping;
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
mod runtime_recall_configuration;
#[cfg(test)]
mod runtime_recall_configuration_test_support;
#[cfg(test)]
mod runtime_recall_configuration_tests;
mod runtime_recall_cutover;
#[cfg(test)]
mod runtime_recall_cutover_tests;
mod runtime_recall_interrupts;
mod runtime_recall_observations;
mod runtime_recall_rerank;
mod runtime_recall_resolution;
mod runtime_recall_state;
#[cfg(test)]
mod runtime_recall_test_support;
#[cfg(test)]
mod runtime_recall_tests;
mod runtime_state;
mod runtime_storage_commands;
mod runtime_subscription_commands;
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
pub use contract_facade::*;
pub use download_cutover_types::{
    LegacyDownloadCutoverCandidate, LegacyDownloadCutoverDisposition, LegacyDownloadCutoverFailure,
    LegacyDownloadCutoverFailureCode, LegacyDownloadCutoverProjection, LegacyDownloadCutoverStage,
};
pub use listening_migration::{
    LegacyListeningBackupEvidence, LegacyListeningImportPlan, LegacyListeningImportReport,
    LegacyListeningImportVerification, LegacyListeningMigrationError, LegacyListeningSourceKind,
    SharedListeningStorePreparation, commit_staged_legacy_listening_import,
    inspect_legacy_listening_source, prepare_shared_listening_store,
    read_staged_legacy_listening_import, shared_store_schema_version,
    stage_legacy_listening_import,
};
pub use model_chapter_cutover_types::{
    LegacyModelChapterCutoverCandidate, LegacyModelChapterCutoverDisposition,
    LegacyModelChapterCutoverFailure, LegacyModelChapterCutoverFailureCode,
    LegacyModelChapterCutoverProjection, LegacyModelChapterCutoverStage,
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

/// Shape of the one native/core API. Durable operation outcomes appear in
/// projections; host observations return only evidence-retention receipts.
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
    fn record_host_observation(
        &self,
        observation: HostObservationEnvelope,
    ) -> HostObservationReceipt;
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
