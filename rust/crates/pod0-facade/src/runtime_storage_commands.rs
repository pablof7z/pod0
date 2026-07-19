use pod0_application::{CoreFailureCode, OperationResult};
use pod0_domain::CommandId;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn finish_storage_command(
        &mut self,
        command_id: CommandId,
        result: Result<pod0_domain::StateRevision, pod0_storage::StorageError>,
        operation_result: OperationResult,
    ) {
        match result {
            Ok(_) => match self
                .reload_listening()
                .and_then(|()| self.reload_notes())
                .and_then(|()| self.reload_clips())
            {
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
        pod0_storage::StorageError::RevisionConflict => CoreFailureCode::RevisionConflict,
        pod0_storage::StorageError::InvalidNote => CoreFailureCode::InvalidNote,
        pod0_storage::StorageError::InvalidClip => CoreFailureCode::InvalidClip,
        pod0_storage::StorageError::InvalidTranscriptArtifact => CoreFailureCode::InvalidTranscript,
        pod0_storage::StorageError::TranscriptRevisionConflict => CoreFailureCode::RevisionConflict,
        pod0_storage::StorageError::TranscriptNotFound => CoreFailureCode::NotFound,
        pod0_storage::StorageError::TranscriptCommandConflict => CoreFailureCode::InvalidCommand,
        _ => CoreFailureCode::StorageUnavailable,
    }
}
