use crate::runtime_command_fingerprint::command_fingerprint;
use crate::runtime_feed_state::FeedIntent;
use crate::runtime_state::FacadeState;
use pod0_application::{ApplicationCommand, CommandEnvelope, CoreFailureCode};

mod listening;

impl FacadeState {
    pub(super) fn accept_command(&mut self, envelope: CommandEnvelope) -> bool {
        self.begin(&envelope);
        let fingerprint = command_fingerprint(&envelope.command);
        match envelope.command.clone() {
            ApplicationCommand::SubscribeToFeed { feed_url } => {
                self.start_feed(&envelope, &fingerprint, feed_url, FeedIntent::Subscribe)
            }
            ApplicationCommand::EnsurePodcast { feed_url } => {
                self.start_feed(&envelope, &fingerprint, feed_url, FeedIntent::Ensure)
            }
            ApplicationCommand::RefreshPodcast { podcast_id } => {
                self.start_refresh(&envelope, &fingerprint, podcast_id)
            }
            ApplicationCommand::HydratePodcastMetadata { podcast_id } => {
                self.start_metadata_refresh(&envelope, &fingerprint, podcast_id)
            }
            ApplicationCommand::UpsertSyntheticPodcast { podcast } => {
                self.upsert_synthetic_podcast(&envelope, &fingerprint, podcast)
            }
            ApplicationCommand::UpsertExternalEpisode { episode } => {
                self.upsert_external_episode(&envelope, &fingerprint, episode)
            }
            ApplicationCommand::Unsubscribe { podcast_id } => {
                self.unsubscribe_podcast(&envelope, &fingerprint, podcast_id)
            }
            ApplicationCommand::SetSubscriptionNotifications {
                podcast_id,
                enabled,
            } => self.set_subscription_notifications(&envelope, &fingerprint, podcast_id, enabled),
            ApplicationCommand::SetSubscriptionAutoDownload { podcast_id, policy } => {
                self.set_subscription_auto_download(&envelope, &fingerprint, podcast_id, policy)
            }
            ApplicationCommand::SetSubscriptionTranscriptStartPolicy { podcast_id, policy } => self
                .set_subscription_transcript_start_policy(
                    &envelope,
                    &fingerprint,
                    podcast_id,
                    policy,
                ),
            ApplicationCommand::SetEpisodeStarred {
                episode_id,
                starred,
            } => self.set_episode_starred(&envelope, &fingerprint, episode_id, starred),
            command @ (ApplicationCommand::RequestEpisodeDownload { .. }
            | ApplicationCommand::ReportAutomaticDownloadCandidates { .. }
            | ApplicationCommand::CancelEpisodeDownload { .. }
            | ApplicationCommand::RemoveEpisodeDownload { .. }
            | ApplicationCommand::ObserveDownloadEnvironment { .. }) => {
                self.accept_download_command(&envelope, &fingerprint, command)
            }
            ApplicationCommand::ResetListeningData => self.reset_all(&envelope, &fingerprint),
            ApplicationCommand::CancelOperation { cancellation_id } => {
                self.cancel_operation(cancellation_id);
                self.succeed(envelope.command_id, None);
            }
            ApplicationCommand::RequestPlayback { .. } => {
                self.fail(envelope.command_id, CoreFailureCode::NotFound)
            }
            ApplicationCommand::Playback { command } => {
                self.accept_playback_command(&envelope, &fingerprint, command)
            }
            ApplicationCommand::RecallQuery { query } => self.start_recall(&envelope, query),
            ApplicationCommand::ImportLegacyRecallConfiguration {
                configuration,
                source_generation,
            } => self.import_legacy_recall_configuration(
                &envelope,
                &fingerprint,
                configuration,
                source_generation,
            ),
            ApplicationCommand::SetRecallConfiguration {
                expected_configuration_revision,
                configuration,
            } => self.set_recall_configuration(
                &envelope,
                &fingerprint,
                expected_configuration_revision,
                configuration,
            ),
            ApplicationCommand::RebuildTranscriptEvidence { input, policy } => {
                self.rebuild_transcript_evidence(&envelope, input, policy);
            }
            ApplicationCommand::CommitRecallIndexCutover => {
                self.start_recall_index_cutover(&envelope);
            }
            ApplicationCommand::CommitTranscript {
                expected_selection_revision,
                artifact,
            } => self.commit_transcript(&envelope, expected_selection_revision, artifact),
            ApplicationCommand::EnsureTranscriptWorkflow {
                episode_id,
                origin,
                configuration,
            } => self.ensure_transcript_workflow(&envelope, episode_id, origin, configuration),
            ApplicationCommand::RetryTranscriptWorkflow {
                episode_id,
                expected_workflow_revision,
                configuration,
            } => self.retry_transcript_workflow(
                &envelope,
                episode_id,
                expected_workflow_revision,
                configuration,
            ),
            ApplicationCommand::CancelTranscriptWorkflow {
                episode_id,
                expected_workflow_revision,
            } => self.cancel_transcript_workflow(&envelope, episode_id, expected_workflow_revision),
            command @ (ApplicationCommand::EnsureScheduledTask { .. }
            | ApplicationCommand::UpdateScheduledTask { .. }
            | ApplicationCommand::RemoveScheduledTask { .. }
            | ApplicationCommand::ReconcileScheduledRuns
            | ApplicationCommand::RetryScheduledRun { .. }
            | ApplicationCommand::CancelScheduledRun { .. }) => {
                self.accept_scheduled_agent_command(&envelope, command)
            }
            command @ (ApplicationCommand::StartAgentTurn { .. }
            | ApplicationCommand::CancelAgentTurn { .. }) => {
                self.accept_agent_command(&envelope, command, &fingerprint)
            }
            ApplicationCommand::PublishGeneratedEpisode { intent } => {
                self.pub_nmp(&envelope, &fingerprint, &intent)
            }
            ApplicationCommand::EnsureNostrSigner => self.ensure_nostr_signer(&envelope),
            ApplicationCommand::SignOutNostrSigner {
                expected_account_id,
            } => self.sign_out_nostr_signer(&envelope, expected_account_id),
            ApplicationCommand::CommitChapter {
                expected_selection_revision,
                artifact,
            } => self.commit_chapter(&envelope, expected_selection_revision, artifact),
            ApplicationCommand::EnsurePublisherChapters { episode_id } => {
                self.ensure_publisher_chapters(&envelope, episode_id)
            }
            ApplicationCommand::RetryPublisherChapters {
                episode_id,
                expected_workflow_revision,
            } => self.retry_publisher_chapters(&envelope, episode_id, expected_workflow_revision),
            ApplicationCommand::CancelPublisherChapters {
                episode_id,
                expected_workflow_revision,
            } => self.cancel_publisher_chapters(&envelope, episode_id, expected_workflow_revision),
            ApplicationCommand::EnsureModelChapters {
                episode_id,
                configured_model,
            } => self.ensure_model_chapters(&envelope, episode_id, configured_model),
            ApplicationCommand::RetryModelChapters {
                episode_id,
                configured_model,
                expected_workflow_revision,
            } => self.retry_model_chapters(
                &envelope,
                episode_id,
                configured_model,
                expected_workflow_revision,
            ),
            ApplicationCommand::CancelModelChapters {
                episode_id,
                expected_workflow_revision,
            } => self.cancel_model_chapters(&envelope, episode_id, expected_workflow_revision),
            ApplicationCommand::CreateNote {
                text,
                kind,
                author,
                target,
            } => self.create_note(&envelope, &fingerprint, &text, kind, author, target),
            ApplicationCommand::UpdateNote {
                note_id,
                expected_note_revision,
                text,
                kind,
                target,
            } => self.update_note(
                &envelope,
                &fingerprint,
                note_id,
                expected_note_revision,
                &text,
                kind,
                target,
            ),
            ApplicationCommand::SetNoteDeleted {
                note_id,
                expected_note_revision,
                deleted,
            } => self.set_note_deleted(
                &envelope,
                &fingerprint,
                note_id,
                expected_note_revision,
                deleted,
            ),
            ApplicationCommand::ClearNotes {
                expected_collection_revision,
            } => self.clear_notes(&envelope, &fingerprint, expected_collection_revision),
            ApplicationCommand::CreateMemory { content } => {
                self.create_memory(&envelope, &fingerprint, &content)
            }
            ApplicationCommand::UpdateMemory {
                memory_id,
                expected_memory_revision,
                content,
            } => self.update_memory(
                &envelope,
                &fingerprint,
                memory_id,
                expected_memory_revision,
                &content,
            ),
            ApplicationCommand::SetMemoryDeleted {
                memory_id,
                expected_memory_revision,
                deleted,
            } => self.set_memory_deleted(
                &envelope,
                &fingerprint,
                memory_id,
                expected_memory_revision,
                deleted,
            ),
            ApplicationCommand::ClearMemories {
                expected_collection_revision,
            } => self.clear_memories(&envelope, &fingerprint, expected_collection_revision),
            ApplicationCommand::CreateClip {
                clip_id,
                episode_id,
                podcast_id,
                start_milliseconds,
                end_milliseconds,
                caption,
                speaker_id,
                frozen_transcript_text,
                source,
            } => self.create_clip(
                &envelope,
                &fingerprint,
                clip_id,
                episode_id,
                podcast_id,
                start_milliseconds,
                end_milliseconds,
                caption.as_deref(),
                speaker_id,
                &frozen_transcript_text,
                source,
            ),
            ApplicationCommand::UpdateClip {
                clip_id,
                expected_clip_revision,
                start_milliseconds,
                end_milliseconds,
                caption,
                speaker_id,
                frozen_transcript_text,
            } => self.update_clip(
                &envelope,
                &fingerprint,
                clip_id,
                expected_clip_revision,
                start_milliseconds,
                end_milliseconds,
                caption.as_deref(),
                speaker_id,
                &frozen_transcript_text,
            ),
            ApplicationCommand::SetClipDeleted {
                clip_id,
                expected_clip_revision,
                deleted,
            } => self.set_clip_deleted(
                &envelope,
                &fingerprint,
                clip_id,
                expected_clip_revision,
                deleted,
            ),
            ApplicationCommand::ClearClips {
                expected_collection_revision,
            } => self.clear_clips(&envelope, &fingerprint, expected_collection_revision),
            ApplicationCommand::Unsupported { wire_code } => {
                self.reject_unsupported(envelope.command_id, wire_code)
            }
        }
        self.trim_operations();
        true
    }
}

include!("runtime_commands_download.rs");
