use pod0_application::{ApplicationCommand, CommandEnvelope, CoreFailureCode, OperationResult};

use crate::runtime_command_fingerprint::command_fingerprint;
use crate::runtime_feed_state::FeedIntent;
use crate::runtime_state::FacadeState;

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
            ApplicationCommand::SetEpisodeStarred {
                episode_id,
                starred,
            } => {
                let result = self
                    .store
                    .as_ref()
                    .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
                    .and_then(|store| {
                        store.set_episode_starred(
                            envelope.command_id,
                            &fingerprint,
                            episode_id,
                            starred,
                            self.now().value,
                        )
                    });
                self.finish_storage_command(
                    envelope.command_id,
                    result,
                    OperationResult::EpisodeUpdated { episode_id },
                );
            }
            ApplicationCommand::RequestEpisodeDownload { episode_id, origin } => {
                self.request_episode_download(&envelope, &fingerprint, episode_id, origin)
            }
            ApplicationCommand::ReportAutomaticDownloadCandidates {
                podcast_id,
                episode_ids,
            } => self.report_automatic_download_candidates(
                &envelope,
                &fingerprint,
                podcast_id,
                episode_ids,
            ),
            ApplicationCommand::CancelEpisodeDownload {
                episode_id,
                expected_workflow_revision,
            } => self.cancel_episode_download(
                &envelope,
                &fingerprint,
                episode_id,
                expected_workflow_revision,
            ),
            ApplicationCommand::RemoveEpisodeDownload {
                episode_id,
                expected_workflow_revision,
            } => self.remove_episode_download(
                &envelope,
                &fingerprint,
                episode_id,
                expected_workflow_revision,
            ),
            ApplicationCommand::ObserveDownloadEnvironment { observation } => {
                self.observe_download_environment(&envelope, &fingerprint, observation)
            }
            ApplicationCommand::ResetListeningData => {
                self.reset_listening_data(&envelope, &fingerprint);
            }
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
            ApplicationCommand::EnsureScheduledTask { .. }
            | ApplicationCommand::UpdateScheduledTask { .. }
            | ApplicationCommand::RemoveScheduledTask { .. }
            | ApplicationCommand::ReconcileScheduledRuns
            | ApplicationCommand::CancelScheduledRun { .. } => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable)
            }
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
            ApplicationCommand::Unsupported { wire_code } => self.fail(
                envelope.command_id,
                CoreFailureCode::Unsupported { wire_code },
            ),
        }
        self.trim_operations();
        true
    }
}
