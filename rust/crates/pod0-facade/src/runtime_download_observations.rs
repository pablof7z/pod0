use pod0_application::{CoreFailureCode, OperationStage};
use pod0_storage::{DownloadHostRequestKind, StoredDownloadStage};

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn finish_download_operation(
        &mut self,
        pending: &pod0_storage::DownloadHostRequestRecord,
        record: &pod0_storage::DownloadWorkflowRecord,
    ) {
        match record.stage {
            StoredDownloadStage::Requested
            | StoredDownloadStage::HostAccepted
            | StoredDownloadStage::Transferring
            | StoredDownloadStage::Staged
            | StoredDownloadStage::RetryScheduled
            | StoredDownloadStage::Removing
            | StoredDownloadStage::Waiting => {
                self.finish(pending.command_id, OperationStage::Running, None, None)
            }
            StoredDownloadStage::Succeeded => self.succeed(pending.command_id, None),
            StoredDownloadStage::Cancelled => {
                if pending.kind == DownloadHostRequestKind::Cancel {
                    self.succeed(pending.command_id, None);
                    self.finish(
                        record.command_id,
                        OperationStage::Cancelled,
                        Some(failure(CoreFailureCode::Cancelled)),
                        None,
                    );
                } else {
                    self.finish(
                        pending.command_id,
                        OperationStage::Cancelled,
                        Some(failure(CoreFailureCode::Cancelled)),
                        None,
                    );
                }
            }
            StoredDownloadStage::Failed => {
                let code = match record.failure_code.as_deref() {
                    Some("offline" | "timed_out" | "transport") => CoreFailureCode::HostUnavailable,
                    Some("permission_denied" | "host_rejected" | "invalid_artifact") => {
                        CoreFailureCode::HostRejected
                    }
                    _ => CoreFailureCode::StorageUnavailable,
                };
                self.fail(pending.command_id, code);
            }
        }
    }
}
