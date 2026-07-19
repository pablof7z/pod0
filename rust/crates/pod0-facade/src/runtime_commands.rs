use pod0_application::{
    ApplicationCommand, CommandEnvelope, CoreFailureCode, OperationResult, OperationStage,
};
use pod0_domain::CommandId;

use crate::runtime_command_fingerprint::command_fingerprint;
use crate::runtime_state::{FacadeState, FeedIntent, failure};

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
            ApplicationCommand::UpsertExternalEpisode {
                podcast_id,
                feed_url,
                podcast_title,
                audio_url,
                title,
                image_url,
                duration_milliseconds,
            } => self.upsert_external_episode(
                &envelope,
                &fingerprint,
                podcast_id,
                feed_url,
                podcast_title,
                audio_url,
                title,
                image_url,
                duration_milliseconds,
            ),
            ApplicationCommand::Unsubscribe { podcast_id } => {
                let result = self
                    .store
                    .as_ref()
                    .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
                    .and_then(|store| {
                        store.unsubscribe(
                            envelope.command_id,
                            &fingerprint,
                            podcast_id,
                            self.now().value,
                        )
                    });
                self.finish_storage_command(
                    envelope.command_id,
                    result,
                    OperationResult::RemovedPodcast { podcast_id },
                );
            }
            ApplicationCommand::SetSubscriptionNotifications {
                podcast_id,
                enabled,
            } => {
                let result = self
                    .store
                    .as_ref()
                    .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
                    .and_then(|store| {
                        store.update_subscription_preferences(
                            envelope.command_id,
                            &fingerprint,
                            podcast_id,
                            None,
                            Some(enabled),
                            self.now().value,
                        )
                    });
                self.finish_storage_command(
                    envelope.command_id,
                    result,
                    OperationResult::PreferencesUpdated { podcast_id },
                );
            }
            ApplicationCommand::SetSubscriptionAutoDownload { podcast_id, policy } => {
                let result = self
                    .store
                    .as_ref()
                    .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
                    .and_then(|store| {
                        store.update_subscription_preferences(
                            envelope.command_id,
                            &fingerprint,
                            podcast_id,
                            Some(policy),
                            None,
                            self.now().value,
                        )
                    });
                self.finish_storage_command(
                    envelope.command_id,
                    result,
                    OperationResult::PreferencesUpdated { podcast_id },
                );
            }
            ApplicationCommand::CancelOperation { cancellation_id } => {
                self.host_requests.cancel(cancellation_id);
                self.host_queue
                    .retain(|request| request.cancellation_id != cancellation_id);
                self.pending_feeds.retain(|_, pending| {
                    self.operations
                        .iter()
                        .find(|operation| operation.command_id == pending.command_id)
                        .is_none_or(|operation| operation.cancellation_id != cancellation_id)
                });
                for operation in &mut self.operations {
                    if operation.cancellation_id == cancellation_id
                        && !operation.stage.is_terminal()
                    {
                        operation.stage = OperationStage::Cancelled;
                        operation.failure = Some(failure(CoreFailureCode::Cancelled));
                    }
                }
                self.succeed(envelope.command_id, None);
            }
            ApplicationCommand::RequestPlayback { .. } => {
                self.fail(envelope.command_id, CoreFailureCode::NotFound)
            }
            ApplicationCommand::Playback { command } => {
                self.accept_playback_command(&envelope, &fingerprint, command)
            }
            ApplicationCommand::Unsupported { wire_code } => self.fail(
                envelope.command_id,
                CoreFailureCode::Unsupported { wire_code },
            ),
        }
        self.trim_operations();
        true
    }

    fn finish_storage_command(
        &mut self,
        command_id: CommandId,
        result: Result<pod0_domain::StateRevision, pod0_storage::StorageError>,
        operation_result: OperationResult,
    ) {
        match result {
            Ok(_) => match self.reload_listening() {
                Ok(()) => self.succeed(command_id, Some(operation_result)),
                Err(error) => self.fail(command_id, storage_failure(error)),
            },
            Err(error) => self.fail(command_id, storage_failure(error)),
        }
    }
}

pub(super) fn storage_failure(error: pod0_storage::StorageError) -> CoreFailureCode {
    match error {
        pod0_storage::StorageError::EntityNotFound => CoreFailureCode::NotFound,
        pod0_storage::StorageError::CommandConflict => CoreFailureCode::InvalidCommand,
        _ => CoreFailureCode::StorageUnavailable,
    }
}
