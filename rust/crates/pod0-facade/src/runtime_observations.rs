use pod0_application::{
    CoreFailureCode, HostFailureCode, HostObservation, HostObservationEnvelope,
    ObservationAcceptance, OperationResult, OperationStage,
};

use crate::runtime_commands::storage_failure;
use crate::runtime_state::{FacadeState, FeedIntent, PendingFeed, failure};

impl FacadeState {
    pub(super) fn record_host_observation(&mut self, observation: HostObservationEnvelope) -> bool {
        let command_id = self.host_requests.command_id(observation.request_id);
        let is_playback_request = self
            .host_requests
            .is_playback_request(observation.request_id);
        if self.host_requests.accept_observation(&observation) != ObservationAcceptance::Accepted {
            return false;
        }
        let Some(command_id) = command_id else {
            return false;
        };
        self.advance_revision();
        if let Some(pending) = self.pending_feeds.remove(&observation.request_id) {
            self.finish_feed_observation(
                pending,
                observation.observation,
                observation.observed_at.value,
            );
        } else {
            match observation.observation {
                HostObservation::Failed { code, .. } if is_playback_request => {
                    let _ = code;
                    self.playback_host_failed(command_id);
                }
                HostObservation::Failed { code, .. } => self.fail(command_id, host_failure(code)),
                HostObservation::Cancelled => self.finish(
                    command_id,
                    OperationStage::Cancelled,
                    Some(failure(CoreFailureCode::Cancelled)),
                    None,
                ),
                HostObservation::PlaybackObserved { value } if is_playback_request => {
                    self.accept_playback_observation(
                        observation.request_id,
                        observation.cancellation_id,
                        observation.sequence_number,
                        observation.observed_at.value,
                        value,
                    );
                }
                HostObservation::PlaybackObserved { .. } => {
                    self.fail(command_id, CoreFailureCode::InvalidCommand)
                }
                HostObservation::FeedBytesFetched { .. }
                | HostObservation::FeedNotModified { .. } => {
                    self.fail(command_id, CoreFailureCode::InvalidCommand)
                }
                HostObservation::Unsupported { wire_code } => {
                    self.fail(command_id, CoreFailureCode::Unsupported { wire_code })
                }
            }
        }
        self.trim_operations();
        true
    }

    fn finish_feed_observation(
        &mut self,
        pending: PendingFeed,
        observation: HostObservation,
        observed_at_ms: i64,
    ) {
        match observation {
            HostObservation::FeedBytesFetched {
                bytes,
                entity_tag,
                last_modified,
                ..
            } => {
                self.apply_fetched_feed(pending, &bytes, entity_tag, last_modified, observed_at_ms)
            }
            HostObservation::FeedNotModified {
                entity_tag,
                last_modified,
                ..
            } => self.apply_not_modified(pending, entity_tag, last_modified, observed_at_ms),
            HostObservation::Failed { code, .. } => {
                self.fail(pending.command_id, host_failure(code))
            }
            HostObservation::Cancelled => self.finish(
                pending.command_id,
                OperationStage::Cancelled,
                Some(failure(CoreFailureCode::Cancelled)),
                None,
            ),
            HostObservation::Unsupported { wire_code } => self.fail(
                pending.command_id,
                CoreFailureCode::Unsupported { wire_code },
            ),
            HostObservation::PlaybackObserved { .. } => {
                self.fail(pending.command_id, CoreFailureCode::InvalidCommand)
            }
        }
    }

    fn apply_fetched_feed(
        &mut self,
        pending: PendingFeed,
        bytes: &[u8],
        entity_tag: Option<String>,
        last_modified: Option<String>,
        observed_at_ms: i64,
    ) {
        let parsed = pod0_application::parse_podcast_feed(
            bytes,
            pending.feed_identity,
            pending.podcast_id,
            pod0_domain::UnixTimestampMilliseconds::new(observed_at_ms),
        );
        let Ok(parsed) = parsed else {
            self.fail(pending.command_id, CoreFailureCode::FeedMalformed);
            return;
        };
        let Some(store) = &self.store else {
            self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let mut episodes = parsed.episodes;
        if pending.intent == FeedIntent::Metadata {
            episodes.clear();
        }
        let result = store.apply_feed(
            pending.command_id,
            &pending.fingerprint,
            parsed.podcast,
            episodes,
            pending.intent == FeedIntent::Subscribe,
            entity_tag,
            last_modified,
            observed_at_ms,
        );
        match result {
            Ok((_, podcast_id)) => match self.reload_listening() {
                Ok(()) => self.succeed(
                    pending.command_id,
                    Some(OperationResult::Podcast { podcast_id }),
                ),
                Err(error) => self.fail(pending.command_id, storage_failure(error)),
            },
            Err(error) => self.fail(pending.command_id, storage_failure(error)),
        }
    }

    fn apply_not_modified(
        &mut self,
        pending: PendingFeed,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        observed_at_ms: i64,
    ) {
        if !matches!(pending.intent, FeedIntent::Refresh | FeedIntent::Metadata) {
            self.fail(pending.command_id, CoreFailureCode::FeedMalformed);
            return;
        }
        let Some(store) = &self.store else {
            self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let result = store.mark_feed_not_modified(
            pending.command_id,
            &pending.fingerprint,
            pending.podcast_id,
            entity_tag,
            last_modified,
            observed_at_ms,
        );
        match result {
            Ok(_) => match self.reload_listening() {
                Ok(()) => self.succeed(
                    pending.command_id,
                    Some(OperationResult::Podcast {
                        podcast_id: pending.podcast_id,
                    }),
                ),
                Err(error) => self.fail(pending.command_id, storage_failure(error)),
            },
            Err(error) => self.fail(pending.command_id, storage_failure(error)),
        }
    }
}

fn host_failure(code: HostFailureCode) -> CoreFailureCode {
    match code {
        HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
        HostFailureCode::InvalidResponse | HostFailureCode::ResponseTooLarge => {
            CoreFailureCode::FeedMalformed
        }
        _ => CoreFailureCode::HostUnavailable,
    }
}
