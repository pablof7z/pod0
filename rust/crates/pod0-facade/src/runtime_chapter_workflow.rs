use pod0_application::{
    HostCancellationRequest, HostRequest, HostRequestEnvelope,
    MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS, MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES,
};
use pod0_domain::UnixTimestampMilliseconds;
use pod0_storage::{PublisherChapterWorkflowRecord, PublisherChapterWorkflowState};

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
            if matches!(
                record.state,
                PublisherChapterWorkflowState::Requested
                    | PublisherChapterWorkflowState::RetryScheduled
            ) {
                self.queue_publisher_chapter_request(record);
            }
        }
        Ok(())
    }

    pub(super) fn admit_publisher_chapter_requests(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let capacity = usize::from(MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS)
            .saturating_sub(self.pending_publisher_chapters.len());
        if capacity == 0 {
            return Ok(());
        }
        let records = self
            .store
            .as_ref()
            .map(|store| {
                store.active_publisher_chapter_workflows(MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS)
            })
            .transpose()?
            .unwrap_or_default();
        for record in records {
            if self.pending_publisher_chapters.len()
                >= usize::from(MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS)
            {
                break;
            }
            self.queue_publisher_chapter_request(record);
        }
        Ok(())
    }

    pub(super) fn queue_publisher_chapter_request(
        &mut self,
        record: PublisherChapterWorkflowRecord,
    ) -> bool {
        let Some(request) = publisher_request(&record) else {
            return false;
        };
        let request_id = request.request_id;
        if self.pending_publisher_chapters.contains_key(&request_id) {
            return true;
        }
        if self.pending_publisher_chapters.len()
            >= usize::from(MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS)
        {
            return true;
        }
        if !self.host_requests.register(request.clone())
            && !self.host_requests.matches_outstanding(&request)
        {
            return false;
        }
        if !self
            .host_queue
            .iter()
            .any(|queued| queued.request_id == request_id)
        {
            self.host_queue.push_back(request);
        }
        self.pending_publisher_chapters.insert(request_id, record);
        true
    }

    pub(super) fn withdraw_publisher_chapter_request(
        &mut self,
        record: &PublisherChapterWorkflowRecord,
    ) {
        if let Some(request_id) = record.request_id {
            let was_queued = self
                .host_queue
                .iter()
                .any(|request| request.request_id == request_id);
            self.host_queue
                .retain(|request| request.request_id != request_id);
            self.pending_publisher_chapters.remove(&request_id);
            self.pending_publisher_observations.remove(&request_id);
            if self.host_requests.cancel_request(request_id) && !was_queued {
                self.host_cancellations.push_back(HostCancellationRequest {
                    request_id,
                    cancellation_id: record.cancellation_id,
                });
            }
            self.host_requests.retire(request_id);
        }
    }

    pub(super) fn retire_publisher_chapter_request(
        &mut self,
        request_id: pod0_domain::HostRequestId,
    ) {
        self.pending_publisher_chapters.remove(&request_id);
        self.pending_publisher_observations.remove(&request_id);
        self.host_requests.retire(request_id);
        let _ = self.admit_publisher_chapter_requests();
    }
}

fn publisher_request(record: &PublisherChapterWorkflowRecord) -> Option<HostRequestEnvelope> {
    if !matches!(
        record.state,
        PublisherChapterWorkflowState::Requested | PublisherChapterWorkflowState::RetryScheduled
    ) {
        return None;
    }
    Some(HostRequestEnvelope {
        request_id: record.request_id?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: record.deadline_at_ms.map(UnixTimestampMilliseconds::new),
        request: HostRequest::FetchPublisherChapters {
            episode_id: record.episode_id,
            source_url: record.source_url.clone(),
            not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
            maximum_response_bytes: MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES as u64,
        },
    })
}
