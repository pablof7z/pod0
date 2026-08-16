use std::cell::RefCell;

use pod0_application::{
    ActivityActor, ActivityOrigin, ChapterTransition, ChapterWorkflowActivityInput,
    ChapterWorkflowMutation, ExternalEffectKind, RequestDisposition,
    plan_chapter_workflow_activity,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    PublisherChapterWorkflowRecord, StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_publisher_chapter_source_absent(
    path: &std::path::Path,
    episode_id: EpisodeId,
    command_id: CommandId,
    now_ms: i64,
    recovery: bool,
) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
    let prior = RefCell::new(None);
    let identity = identity(command_id, episode_id, recovery);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: if recovery {
                TransitionIngressKind::Recovery
            } else {
                TransitionIngressKind::ApplicationCommand
            },
            id: identity.into_bytes(),
            fingerprint: fingerprint(episode_id, recovery),
        },
        UnixTimestampMilliseconds::new(now_ms.max(0)),
        |transaction| {
            let existing =
                crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?;
            *prior.borrow_mut() = existing.clone();
            let current_revision = existing
                .as_ref()
                .map_or(pod0_domain::StateRevision::INITIAL, |record| {
                    record.workflow_revision
                });
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                identity_command_id: identity,
                command_id,
                episode_id,
                current_revision,
                disposition: if existing.is_some() {
                    RequestDisposition::Accepted
                } else {
                    RequestDisposition::NoSemanticChange
                },
                transition: existing
                    .is_some()
                    .then_some(ChapterTransition::PublisherWorkflowStateChanged),
                effect: None,
                effect_kind: ExternalEffectKind::PublisherChapterProvider,
                actor: if recovery {
                    ActivityActor::Recovery
                } else {
                    ActivityActor::User
                },
                origin: if recovery {
                    ActivityOrigin::Recovery
                } else {
                    ActivityOrigin::UserInterface
                },
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            ChapterWorkflowMutation::Apply => {
                let changed = transaction
                    .execute(
                        "UPDATE pod0_publisher_chapter_workflows SET state='source_absent',\
                         workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                         not_before_ms=NULL,failure_code=NULL,failure_detail=NULL,updated_at_ms=?1 \
                         WHERE episode_id=?2 AND workflow_revision=?3",
                        rusqlite::params![
                            now_ms,
                            episode_id.into_bytes().as_slice(),
                            i64::try_from(expected.value)
                                .map_err(|_| StorageError::ChapterWorkflowConflict)?,
                        ],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("mark publisher chapter source absent", error)
                    })?;
                if changed != 1 {
                    return Err(StorageError::ChapterWorkflowConflict);
                }
                Ok(pod0_domain::StateRevision::new(
                    expected
                        .value
                        .checked_add(1)
                        .ok_or(StorageError::ChapterWorkflowConflict)?,
                ))
            }
            ChapterWorkflowMutation::RecordNoChange => Ok(expected),
        },
    )?;
    if receipt.replayed {
        return crate::LibraryStore::open_authoritative(path)?
            .publisher_chapter_workflow(episode_id);
    }
    Ok(prior.into_inner())
}

fn identity(command_id: CommandId, episode_id: EpisodeId, recovery: bool) -> CommandId {
    if !recovery {
        return command_id;
    }
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/source-absent/recovery/v1");
    hash.update(command_id.into_bytes());
    hash.update(episode_id.into_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn fingerprint(episode_id: EpisodeId, recovery: bool) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/source-absent/v1");
    hash.update(episode_id.into_bytes());
    hash.update([u8::from(recovery)]);
    ContentDigest::from_bytes(hash.finalize().into())
}
