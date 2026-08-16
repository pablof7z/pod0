use pod0_application::{ApplicationCommand, CommandEnvelope, CoreFailureCode};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reject_missing_playback_episode(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: pod0_domain::EpisodeId,
    ) {
        self.reject_application_request(
            envelope,
            pod0_application::ActivitySubject::Episode { episode_id },
            Some(episode_id),
            pod0_application::RequestRejectionReason::MissingSubject,
            CoreFailureCode::NotFound,
        );
    }

    pub(super) fn reject_unsupported_command(&mut self, envelope: &CommandEnvelope, wire_code: u32) {
        self.reject_application_request(
            envelope,
            pod0_application::ActivitySubject::Global,
            None,
            pod0_application::RequestRejectionReason::UnsupportedCode { wire_code },
            CoreFailureCode::Unsupported { wire_code },
        );
    }

    pub(super) fn accept_library_network_command(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        command: ApplicationCommand,
    ) {
        let intent = match command {
            ApplicationCommand::SearchPodcastDirectory { query, limit } => {
                pod0_application::LibraryNetworkIntent::DirectorySearch { query, limit }
            }
            ApplicationCommand::LoadTopPodcasts { storefront, limit } => {
                pod0_application::LibraryNetworkIntent::TopPodcasts { storefront, limit }
            }
            ApplicationCommand::ImportSharedEpisode { source_url } => {
                pod0_application::LibraryNetworkIntent::SharedEpisodeImport { source_url }
            }
            ApplicationCommand::SearchPodcastCatalog {
                episode_query,
                podcast_hint,
                limit,
            } => pod0_application::LibraryNetworkIntent::CatalogEpisodeSearch {
                episode_query,
                podcast_hint,
                limit,
            },
            _ => return self.fail(envelope.command_id, CoreFailureCode::InvalidCommand),
        };
        let Some(store) = self.store.clone() else {
            return self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
        };
        let now = self.now();
        let result = store.admit_library_network(pod0_storage::LibraryNetworkAdmissionInput {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            command_fingerprint: fingerprint.to_owned(),
            fingerprint: crate::runtime_command_fingerprint::command_fingerprint_digest(
                &envelope.command,
            ),
            intent,
            now_ms: now.value,
            deadline_at_ms: now.value.saturating_add(30_000),
        });
        match result {
            Ok(record) => {
                self.revision = record.revision;
                if record.stage == pod0_storage::StoredLibraryNetworkStage::Completed {
                    self.succeed(
                        envelope.command_id,
                        record.result.map(|value| match value {
                            pod0_storage::StoredLibraryNetworkResult::Directory { entries } => {
                                pod0_application::OperationResult::PodcastDirectoryResults {
                                    results: entries,
                                }
                            }
                            pod0_storage::StoredLibraryNetworkResult::SharedEpisode { episode_id } => {
                                pod0_application::OperationResult::SharedEpisodeImported { episode_id }
                            }
                            pod0_storage::StoredLibraryNetworkResult::Catalog {
                                episode_ids,
                                bounded_result,
                            } => pod0_application::OperationResult::PodcastCatalogResults {
                                episode_ids,
                                bounded_result,
                            },
                        }),
                    );
                }
            }
            Err(pod0_storage::StorageError::InvalidActivity) => {
                self.fail(envelope.command_id, CoreFailureCode::InvalidCommand)
            }
            Err(_) => self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable),
        }
    }
}
