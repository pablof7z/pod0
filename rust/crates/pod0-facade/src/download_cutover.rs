use std::collections::BTreeSet;

use pod0_application::{
    DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS, download_attempt_id, download_input_version,
    download_intent_id,
};
use pod0_domain::{CancellationId, CommandId};
use pod0_storage::{
    LegacyDownloadCutoverDisposition as StoredDisposition, LegacyDownloadCutoverEntry,
    LegacyDownloadCutoverInput, StorageError, download_start_request_id,
};
use sha2::{Digest as _, Sha256};

use crate::Pod0Facade;
use crate::download_cutover_types::{
    LegacyDownloadCutoverCandidate, LegacyDownloadCutoverDisposition,
    LegacyDownloadCutoverProjection, LegacyDownloadCutoverStage,
};
use crate::runtime_download_mapping::stored_origin;

#[uniffi::export]
impl Pod0Facade {
    pub fn download_cutover(&self) -> LegacyDownloadCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyDownloadCutoverProjection::blocked(
                StorageError::DownloadWorkflowConflict,
            );
        };
        store
            .download_cutover_report()
            .map(LegacyDownloadCutoverProjection::from_report)
            .unwrap_or_else(LegacyDownloadCutoverProjection::blocked)
    }

    pub fn stage_legacy_download_cutover(
        &self,
        source_generation: u64,
        candidates: Vec<LegacyDownloadCutoverCandidate>,
    ) -> LegacyDownloadCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyDownloadCutoverProjection::blocked(
                    StorageError::DownloadWorkflowConflict,
                );
            };
            let now = state.now().value;
            let Some(deadline) = now.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS)
            else {
                return LegacyDownloadCutoverProjection::blocked(
                    StorageError::DownloadWorkflowConflict,
                );
            };
            let entries = match entries(&state.listening.episodes, source_generation, candidates) {
                Ok(entries) => entries,
                Err(error) => return LegacyDownloadCutoverProjection::blocked(error),
            };
            let result = store.stage_legacy_download_cutover(LegacyDownloadCutoverInput {
                source_generation,
                entries,
                issued_revision: state.revision,
                now_ms: now,
                deadline_at_ms: deadline,
            });
            if result.is_ok() {
                let _ = state.reload_listening();
                state.advance_revision();
            }
            result
        };
        match result {
            Ok(report) => {
                self.notify_subscribers();
                LegacyDownloadCutoverProjection::from_report(report)
            }
            Err(error) => LegacyDownloadCutoverProjection::blocked(error),
        }
    }

    pub fn commit_legacy_download_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyDownloadCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyDownloadCutoverProjection::blocked(
                    StorageError::DownloadWorkflowConflict,
                );
            };
            match store.commit_legacy_download_cutover(source_generation, state.now().value) {
                Ok(authority) => {
                    let _ = state.reload_listening();
                    state.advance_revision();
                    let _ = state.rehydrate_download_workflows();
                    Ok(authority)
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(authority) => {
                self.notify_subscribers();
                let mut projection = self.download_cutover();
                if projection.stage == LegacyDownloadCutoverStage::Blocked {
                    projection = LegacyDownloadCutoverProjection::from_authority(authority);
                }
                projection
            }
            Err(error) => LegacyDownloadCutoverProjection::blocked(error),
        }
    }

    pub fn discard_staged_legacy_download_cutover(
        &self,
        source_generation: u64,
        candidates: Vec<LegacyDownloadCutoverCandidate>,
    ) -> LegacyDownloadCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyDownloadCutoverProjection::blocked(
                    StorageError::DownloadWorkflowConflict,
                );
            };
            let now = state.now().value;
            let Some(deadline) = now.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS)
            else {
                return LegacyDownloadCutoverProjection::blocked(
                    StorageError::DownloadWorkflowConflict,
                );
            };
            let entries = match entries(&state.listening.episodes, source_generation, candidates) {
                Ok(entries) => entries,
                Err(error) => return LegacyDownloadCutoverProjection::blocked(error),
            };
            let result = store.discard_staged_legacy_download_cutover(LegacyDownloadCutoverInput {
                source_generation,
                entries,
                issued_revision: state.revision,
                now_ms: now,
                deadline_at_ms: deadline,
            });
            if result.is_ok() {
                let _ = state.reload_listening();
                state.advance_revision();
            }
            result
        };
        match result {
            Ok(authority) => {
                self.notify_subscribers();
                LegacyDownloadCutoverProjection::from_authority(authority)
            }
            Err(error) => LegacyDownloadCutoverProjection::blocked(error),
        }
    }
}

fn entries(
    episodes: &[pod0_domain::EpisodeRecord],
    source_generation: u64,
    candidates: Vec<LegacyDownloadCutoverCandidate>,
) -> Result<Vec<LegacyDownloadCutoverEntry>, StorageError> {
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .map(|candidate| {
            if !seen.insert(candidate.episode_id) {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            let episode = episodes
                .iter()
                .find(|episode| episode.episode_id == candidate.episode_id)
                .ok_or(StorageError::DownloadWorkflowConflict)?;
            let input_version = download_input_version(
                &episode.enclosure_url,
                episode.enclosure_mime_type.as_deref(),
                episode.duration_milliseconds,
            )
            .ok_or(StorageError::DownloadWorkflowConflict)?;
            let intent_id = download_intent_id(candidate.episode_id, &input_version)
                .ok_or(StorageError::DownloadWorkflowConflict)?;
            let attempt_id =
                download_attempt_id(intent_id, 1).ok_or(StorageError::DownloadWorkflowConflict)?;
            let disposition = match candidate.disposition {
                LegacyDownloadCutoverDisposition::Available {
                    source_path,
                    byte_count,
                } => StoredDisposition::Available {
                    source_path,
                    byte_count,
                },
                LegacyDownloadCutoverDisposition::Restart { resume_available } => {
                    StoredDisposition::Restart { resume_available }
                }
            };
            Ok(LegacyDownloadCutoverEntry {
                episode_id: candidate.episode_id,
                intent_id,
                attempt_id,
                request_id: download_start_request_id(attempt_id),
                input_version,
                enclosure_url: episode.enclosure_url.clone(),
                origin: stored_origin(candidate.origin)
                    .map_err(|_| StorageError::DownloadWorkflowConflict)?,
                command_id: stable_id(
                    b"pod0-legacy-download-command-v1",
                    source_generation,
                    candidate.episode_id.into_bytes(),
                ),
                cancellation_id: CancellationId::from_bytes(stable_bytes(
                    b"pod0-legacy-download-cancellation-v1",
                    source_generation,
                    candidate.episode_id.into_bytes(),
                )),
                disposition,
            })
        })
        .collect()
}

fn stable_id(domain: &[u8], generation: u64, episode: [u8; 16]) -> CommandId {
    CommandId::from_bytes(stable_bytes(domain, generation, episode))
}

fn stable_bytes(domain: &[u8], generation: u64, episode: [u8; 16]) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    hash.update(generation.to_be_bytes());
    hash.update(episode);
    hash.finalize()[..16].try_into().expect("digest prefix")
}
