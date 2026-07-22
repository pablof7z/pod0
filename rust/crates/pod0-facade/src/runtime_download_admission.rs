use pod0_application::{
    DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS, DownloadAdmissionDecision,
    DownloadEnvironmentObservation, evaluate_download_admission,
};
use pod0_domain::{AutoDownloadMode, AutoDownloadPolicy};
use pod0_storage::StoredDownloadStage;

use crate::runtime_download_mapping::{environment_projection, projected_origin};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reconcile_download_admission(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let Some(store) = self.store.as_ref() else {
            return Ok(());
        };
        self.reconcile_waiting_downloads(environment_projection(store.download_environment()?))
    }

    pub(super) fn reconcile_waiting_downloads(
        &mut self,
        environment: DownloadEnvironmentObservation,
    ) -> Result<(), pod0_storage::StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let page = store.download_workflow_page(
            None,
            0,
            pod0_application::MAX_ACTIVE_DOWNLOAD_WORKFLOWS,
        )?;
        for workflow in page
            .items
            .into_iter()
            .filter(|workflow| workflow.stage == StoredDownloadStage::Waiting)
        {
            let Some(episode) = self
                .listening
                .episodes
                .iter()
                .find(|episode| episode.episode_id == workflow.episode_id)
            else {
                continue;
            };
            let decision = evaluate_download_admission(
                projected_origin(workflow.origin),
                self.download_policy(episode.podcast_id),
                environment,
            );
            let now = self.now().value;
            let transition = match decision {
                DownloadAdmissionDecision::Admit => {
                    let Some(deadline) =
                        now.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS)
                    else {
                        continue;
                    };
                    store.admit_waiting_download(
                        workflow.episode_id,
                        workflow.workflow_revision,
                        self.revision,
                        now,
                        deadline,
                    )?
                }
                DownloadAdmissionDecision::Obsolete => store.retire_obsolete_waiting_download(
                    workflow.episode_id,
                    workflow.workflow_revision,
                    now,
                )?,
                DownloadAdmissionDecision::Wait { .. } => continue,
            };
            self.finish_download_command(transition.record.command_id, transition.record);
        }
        Ok(())
    }

    pub(super) fn download_policy(&self, podcast_id: pod0_domain::PodcastId) -> AutoDownloadPolicy {
        self.listening
            .subscriptions
            .iter()
            .find(|item| item.podcast_id == podcast_id)
            .map_or(
                AutoDownloadPolicy {
                    mode: AutoDownloadMode::Off,
                    wifi_only: false,
                },
                |item| item.auto_download,
            )
    }
}
