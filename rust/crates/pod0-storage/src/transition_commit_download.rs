use pod0_application::{
    ActivityDomain, DownloadAdmissionActivityInput, DownloadIntentOrigin,
    DownloadInternalAdmissionActivityInput, InternalCommandKind, plan_download_admission,
    plan_download_internal_admission,
};
use pod0_domain::{ContentDigest, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::legacy_library_receipt;
use crate::download_store_read::workflow;
use crate::download_store_write::apply_download_ensure;
use crate::{
    DownloadEnsureInput, DownloadEnsureOutcome, DownloadWorkflowRecord, StorageError,
    StoredDownloadOrigin, StoredDownloadStage, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_download_admission(
    path: &std::path::Path,
    input: DownloadEnsureInput,
) -> Result<DownloadEnsureOutcome, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    let command_id = input.command_id;
    let episode_id = input.episode_id;
    let observed_at = input.now_ms;
    let fingerprint = fingerprint(&input.command_fingerprint)?;
    let planning_input = input.clone();
    let existing = std::cell::RefCell::new(None);
    let state_changes = std::cell::Cell::new(false);
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint,
    };
    let receipt = TransitionCommit::open(path)?
        .commit_planned_with(
            ingress,
            UnixTimestampMilliseconds::new(observed_at),
            |transaction| {
                let prior = workflow(transaction, episode_id)?;
                let legacy_replay = legacy_library_receipt(
                    transaction,
                    command_id,
                    &planning_input.command_fingerprint,
                    "read download command",
                )?
                .is_some();
                let changes = !legacy_replay && !is_semantic_noop(prior.as_ref(), &planning_input);
                let current = prior
                    .as_ref()
                    .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
                let effect = (changes && planning_input.admitted)
                    .then(|| {
                        crate::download_effect_request::start_for_ensure(
                            prior.as_ref(),
                            &planning_input,
                        )
                    })
                    .transpose()?
                    .map(|request| pod0_application::DownloadEffectAuthorization { request });
                *existing.borrow_mut() = prior;
                state_changes.set(changes);
                plan_download_admission(DownloadAdmissionActivityInput {
                    command_id,
                    episode_id,
                    current_revision: current,
                    legacy_replay,
                    state_changes: changes,
                    admitted: planning_input.admitted,
                    effect,
                    origin: origin(planning_input.origin),
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, ()| {
                let in_transaction = workflow(transaction, episode_id)?;
                let actual = in_transaction
                    .as_ref()
                    .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
                if actual != expected {
                    return Err(StorageError::RevisionConflict);
                }
                match apply_download_ensure(transaction, input)? {
                    DownloadEnsureOutcome::Changed { record, .. } => {
                        retire_download_effects(transaction, episode_id)?;
                        Ok(record.workflow_revision)
                    }
                    DownloadEnsureOutcome::Existing(record) => Ok(record.workflow_revision),
                }
            },
        )
        .map_err(download_command_error)?;
    let record = store
        .download_workflow(episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    if state_changes.get() && !receipt.replayed {
        Ok(DownloadEnsureOutcome::Changed {
            record,
            replaced: existing.borrow_mut().take().map(Box::new),
        })
    } else {
        Ok(DownloadEnsureOutcome::Existing(record))
    }
}

pub(crate) fn commit_download_internal_admission(
    path: &std::path::Path,
    command: crate::PendingInternalCommand,
    input: DownloadEnsureInput,
) -> Result<DownloadEnsureOutcome, StorageError> {
    let InternalCommandKind::RequestEpisodeDownload {
        origin: authorized_origin,
    } = command.request.kind
    else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::Download
        || command.request.episode_id != Some(input.episode_id)
        || authorized_origin != origin(input.origin)
        || command.internal_command_id.into_bytes() != input.command_id.into_bytes()
    {
        return Err(StorageError::InvalidActivity);
    }
    let store = crate::LibraryStore::open_authoritative(path)?;
    let episode_id = input.episode_id;
    let observed_at = input.now_ms;
    let planning_input = input.clone();
    let existing = std::cell::RefCell::new(None);
    let state_changes = std::cell::Cell::new(false);
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::InternalCommand,
        id: command.internal_command_id.into_bytes(),
        fingerprint: fingerprint(&input.command_fingerprint)?,
    };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at),
        |transaction| {
            let prior = workflow(transaction, episode_id)?;
            let changes = !is_semantic_noop(prior.as_ref(), &planning_input);
            let current = prior
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            let effect = (changes && planning_input.admitted)
                .then(|| {
                    crate::download_effect_request::start_for_ensure(
                        prior.as_ref(),
                        &planning_input,
                    )
                })
                .transpose()?
                .map(|request| pod0_application::DownloadEffectAuthorization { request });
            *existing.borrow_mut() = prior;
            state_changes.set(changes);
            plan_download_internal_admission(DownloadInternalAdmissionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                episode_id,
                current_revision: current,
                state_changes: changes,
                admitted: planning_input.admitted,
                effect,
                disposition: if changes {
                    pod0_application::RequestDisposition::Accepted
                } else {
                    pod0_application::RequestDisposition::NoSemanticChange
                },
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            let actual = workflow(transaction, episode_id)?
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            if actual != expected {
                return Err(StorageError::RevisionConflict);
            }
            match apply_download_ensure(transaction, input)? {
                DownloadEnsureOutcome::Changed { record, .. } => {
                    retire_download_effects(transaction, episode_id)?;
                    Ok(record.workflow_revision)
                }
                DownloadEnsureOutcome::Existing(record) => Ok(record.workflow_revision),
            }
        },
    )?;
    let record = store
        .download_workflow(episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    if state_changes.get() && !receipt.replayed {
        Ok(DownloadEnsureOutcome::Changed {
            record,
            replaced: existing.borrow_mut().take().map(Box::new),
        })
    } else {
        Ok(DownloadEnsureOutcome::Existing(record))
    }
}

#[allow(clippy::too_many_arguments)]
fn is_semantic_noop(
    existing: Option<&DownloadWorkflowRecord>,
    input: &DownloadEnsureInput,
) -> bool {
    existing.is_some_and(|record| {
        record.intent_id == input.intent_id
            && (record.stage.is_active()
                || record.stage == StoredDownloadStage::Succeeded
                || (!input.admitted && record.stage == StoredDownloadStage::Waiting))
    })
}

pub(super) fn retire_download_effects(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: pod0_domain::EpisodeId,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=3 WHERE state_code=1 AND intent_id IN(\
             SELECT intent_id FROM pod0_effect_intents WHERE episode_id=?1 \
             AND json_extract(request_json,'$.kind')='Download' AND state_code IN(1,2))",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire download effect attempts", error))?;
    transaction
        .execute(
            "UPDATE pod0_effect_intents SET state_code=3 WHERE episode_id=?1 \
             AND json_extract(request_json,'$.kind')='Download' AND state_code IN(1,2)",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire download effects", error))?;
    Ok(())
}

const fn origin(value: StoredDownloadOrigin) -> DownloadIntentOrigin {
    match value {
        StoredDownloadOrigin::User => DownloadIntentOrigin::User,
        StoredDownloadOrigin::Playback => DownloadIntentOrigin::Playback,
        StoredDownloadOrigin::Automatic => DownloadIntentOrigin::Automatic,
        StoredDownloadOrigin::Unsupported(wire_code) => {
            DownloadIntentOrigin::Unsupported { wire_code }
        }
    }
}

fn fingerprint(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 {
        return Err(StorageError::DownloadCommandConflict);
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| StorageError::DownloadCommandConflict)?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}

fn download_command_error(error: StorageError) -> StorageError {
    match error {
        StorageError::ActivityCommandConflict => StorageError::DownloadCommandConflict,
        other => other,
    }
}
