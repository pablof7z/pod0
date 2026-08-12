use pod0_domain::{ChapterArtifact, ChapterArtifactInput, CommandId, ContentDigest, StateRevision};

use crate::{ChapterCommitStorageReceipt, LibraryStore, StorageError};

impl LibraryStore {
    pub fn commit_and_select_chapter(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: ChapterArtifactInput,
        completed_at_ms: i64,
    ) -> Result<ChapterCommitStorageReceipt, StorageError> {
        let artifact = ChapterArtifact::seal(input.clone())
            .map_err(|_| StorageError::InvalidChapterArtifact)?;
        crate::transition_commit::commit_chapter_artifact(
            self.path(),
            command_id,
            artifact.command_fingerprint(expected_selection_revision),
            expected_selection_revision,
            input,
            completed_at_ms,
        )
    }

    pub fn commit_and_select_chapter_with_activity_fingerprint(
        &self,
        command_id: CommandId,
        activity_fingerprint: ContentDigest,
        expected_selection_revision: StateRevision,
        input: ChapterArtifactInput,
        completed_at_ms: i64,
    ) -> Result<ChapterCommitStorageReceipt, StorageError> {
        crate::transition_commit::commit_chapter_artifact(
            self.path(),
            command_id,
            activity_fingerprint,
            expected_selection_revision,
            input,
            completed_at_ms,
        )
    }
}
