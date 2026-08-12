use pod0_application::{
    ChapterArtifactActivityInput, ChapterArtifactMutation, RequestDisposition,
    RequestRejectionReason, plan_chapter_artifact_commit,
};
use pod0_domain::{
    ChapterArtifact, ChapterArtifactInput, CommandId, ContentDigest, StateRevision,
    UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use crate::library_store_chapters::{
    commit_and_select_chapter_in_transaction, current_selection, replay, require_episode_parent,
    require_selected_transcript_provenance,
};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_chapter_artifact(
    path: &std::path::Path,
    command_id: CommandId,
    activity_fingerprint: ContentDigest,
    expected_selection_revision: StateRevision,
    input: ChapterArtifactInput,
    completed_at_ms: i64,
) -> Result<crate::ChapterCommitStorageReceipt, StorageError> {
    let episode_id = input.episode_id;
    let sealed = ChapterArtifact::seal(input);
    let store = crate::LibraryStore::open_authoritative(path)?;
    let artifact = sealed.ok();
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: activity_fingerprint,
    };
    let receipt = TransitionCommit::open(path)?
        .commit_planned_with(
            ingress,
            UnixTimestampMilliseconds::new(completed_at_ms.max(0)),
            |transaction| {
                let current = current_selection(transaction, episode_id)?
                    .map_or(StateRevision::INITIAL, |item| item.1);
                let artifact_is_valid = match artifact.as_ref() {
                    Some(artifact) if completed_at_ms >= 0 && artifact.generated_at.value >= 0 => {
                        match require_episode_parent(transaction, artifact) {
                            Ok(()) => true,
                            Err(StorageError::InvalidChapterArtifact) => false,
                            Err(error) => return Err(error),
                        }
                    }
                    _ => false,
                };
                let provenance_is_current = match artifact.as_ref() {
                    Some(artifact) => {
                        match require_selected_transcript_provenance(transaction, artifact) {
                            Ok(()) => true,
                            Err(StorageError::ChapterRevisionConflict) => false,
                            Err(error) => return Err(error),
                        }
                    }
                    None => false,
                };
                let legacy = artifact
                    .as_ref()
                    .map(|artifact| {
                        replay(
                            transaction,
                            command_id,
                            artifact.command_fingerprint(expected_selection_revision),
                            artifact,
                        )
                    })
                    .transpose()?
                    .flatten();
                plan_chapter_artifact_commit(ChapterArtifactActivityInput {
                    command_id,
                    episode_id,
                    current_selection_revision: current,
                    expected_selection_revision,
                    legacy_replay: legacy.is_some(),
                    artifact_is_valid,
                    transcript_provenance_is_current: provenance_is_current,
                })
                .map(|plan| plan.map_mutation(|mutation| (mutation, legacy)))
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, (mutation, legacy)| match mutation {
                ChapterArtifactMutation::Commit => {
                    let artifact = artifact
                        .as_ref()
                        .ok_or(StorageError::InvalidChapterArtifact)?;
                    Ok(commit_and_select_chapter_in_transaction(
                        transaction,
                        command_id,
                        expected_selection_revision,
                        artifact,
                        completed_at_ms,
                        || Ok(()),
                    )?
                    .selection_revision)
                }
                ChapterArtifactMutation::RecordRejection => {
                    let actual = current_selection(transaction, episode_id)?
                        .map_or(StateRevision::INITIAL, |item| item.1);
                    if actual != expected {
                        return Err(StorageError::RevisionConflict);
                    }
                    Ok(expected)
                }
                ChapterArtifactMutation::LegacyDuplicate => Ok(legacy
                    .as_ref()
                    .expect("planned legacy chapter replay")
                    .selection_revision),
            },
        )
        .map_err(|error| match error {
            StorageError::ActivityCommandConflict => StorageError::ChapterCommandConflict,
            other => other,
        })?;
    match receipt.disposition {
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        } => Err(StorageError::ChapterRevisionConflict),
        RequestDisposition::Rejected { .. } => Err(StorageError::InvalidChapterArtifact),
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            let artifact = artifact.ok_or(StorageError::InvalidChapterArtifact)?;
            store
                .read(|connection| {
                    replay(
                        connection,
                        command_id,
                        artifact.command_fingerprint(expected_selection_revision),
                        &artifact,
                    )
                })?
                .ok_or(StorageError::ChapterCommandConflict)
        }
        _ => Err(StorageError::InvalidActivity),
    }
}
