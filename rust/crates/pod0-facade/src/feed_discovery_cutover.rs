use pod0_domain::ContentDigest;
use pod0_storage::{
    LegacyFeedDiscoveryCutoverInput, StorageError, inspect_legacy_feed_discovery_cutover,
};

use crate::feed_discovery_cutover_mapping::cutover_input;
use crate::{LegacyFeedDiscoveryCandidateInput, LegacyFeedDiscoveryCutoverProjection, Pod0Facade};

#[uniffi::export]
impl Pod0Facade {
    pub fn feed_discovery_cutover(&self) -> LegacyFeedDiscoveryCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyFeedDiscoveryCutoverProjection::blocked(StorageError::Io {
                operation: "feed_discovery_cutover_store",
            });
        };
        store
            .feed_discovery_cutover_report()
            .map(LegacyFeedDiscoveryCutoverProjection::from_report)
            .unwrap_or_else(LegacyFeedDiscoveryCutoverProjection::blocked)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn inspect_legacy_feed_discovery_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        notifications_enabled: bool,
        inspected_job_count: u32,
        blocked_count: u32,
        candidates: Vec<LegacyFeedDiscoveryCandidateInput>,
    ) -> LegacyFeedDiscoveryCutoverProjection {
        let state = self.state();
        let input = match cutover_input(
            backup_digest,
            backup_byte_count,
            notifications_enabled,
            inspected_job_count,
            blocked_count,
            candidates,
            state.now(),
        ) {
            Ok(input) => input,
            Err(error) => return LegacyFeedDiscoveryCutoverProjection::blocked(error),
        };
        inspected_projection(&input)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stage_legacy_feed_discovery_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        notifications_enabled: bool,
        inspected_job_count: u32,
        blocked_count: u32,
        candidates: Vec<LegacyFeedDiscoveryCandidateInput>,
    ) -> LegacyFeedDiscoveryCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyFeedDiscoveryCutoverProjection::blocked(StorageError::Io {
                    operation: "feed_discovery_cutover_store",
                });
            };
            let input = match cutover_input(
                backup_digest,
                backup_byte_count,
                notifications_enabled,
                inspected_job_count,
                blocked_count,
                candidates,
                state.now(),
            ) {
                Ok(input) => input,
                Err(error) => return LegacyFeedDiscoveryCutoverProjection::blocked(error),
            };
            let result = store.stage_legacy_feed_discovery_cutover(input);
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.feed_discovery_cutover_result(result)
    }

    pub fn commit_legacy_feed_discovery_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyFeedDiscoveryCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyFeedDiscoveryCutoverProjection::blocked(StorageError::Io {
                    operation: "feed_discovery_cutover_store",
                });
            };
            let result = store.commit_legacy_feed_discovery_cutover(source_generation, state.now());
            match result {
                Ok(report) => {
                    state.new_episode_notification_settings = match store
                        .new_episode_notification_settings()
                    {
                        Ok(settings) => settings,
                        Err(error) => return LegacyFeedDiscoveryCutoverProjection::blocked(error),
                    };
                    state.listening = match store.snapshot() {
                        Ok(snapshot) => snapshot,
                        Err(error) => return LegacyFeedDiscoveryCutoverProjection::blocked(error),
                    };
                    if let Err(error) = state.rehydrate_feed_discovery_workflows() {
                        Err(error)
                    } else {
                        state.advance_revision();
                        Ok(report)
                    }
                }
                Err(error) => Err(error),
            }
        };
        self.feed_discovery_cutover_result(result)
    }
}

impl Pod0Facade {
    fn feed_discovery_cutover_result(
        &self,
        result: Result<pod0_storage::LegacyFeedDiscoveryCutoverReport, StorageError>,
    ) -> LegacyFeedDiscoveryCutoverProjection {
        match result {
            Ok(report) => {
                self.notify_subscribers();
                LegacyFeedDiscoveryCutoverProjection::from_report(report)
            }
            Err(error) => LegacyFeedDiscoveryCutoverProjection::blocked(error),
        }
    }
}

fn inspected_projection(
    input: &LegacyFeedDiscoveryCutoverInput,
) -> LegacyFeedDiscoveryCutoverProjection {
    match inspect_legacy_feed_discovery_cutover(input) {
        Ok((fingerprint, generation)) => {
            LegacyFeedDiscoveryCutoverProjection::inspected(input, fingerprint, generation)
        }
        Err(error) => LegacyFeedDiscoveryCutoverProjection::blocked(error),
    }
}
