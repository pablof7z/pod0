use pod0_application::MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS;
use pod0_storage::PublisherChapterWorkflowRecord;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn rehydrate_publisher_chapter_workflows(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let records = self
            .store
            .as_ref()
            .map(|store| {
                store.active_publisher_chapter_workflows(MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS)
            })
            .transpose()?
            .unwrap_or_default();
        for record in records {
            self.revision = pod0_domain::StateRevision::new(
                self.revision.value.max(record.workflow_revision.value),
            );
        }
        Ok(())
    }

    pub(super) fn withdraw_publisher_chapter_request(
        &mut self,
        record: &PublisherChapterWorkflowRecord,
    ) {
        if let Some(request_id) = record.request_id {
            self.host_requests.cancel_request(request_id);
            self.host_requests.retire(request_id);
        }
    }

    pub(super) fn retire_publisher_chapter_request(
        &mut self,
        request_id: pod0_domain::HostRequestId,
    ) {
        self.host_requests.retire(request_id);
    }
}
