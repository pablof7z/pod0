use pod0_application::{
    RequestDisposition, RequestRejectionReason, TranscriptCutoverActivityInput,
    TranscriptCutoverMutation, plan_transcript_cutover,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::legacy_transcript_source::inspect_transcript_source;
use crate::transcript_import_model::TranscriptImportState;
use crate::transcript_import_store_read::{open_current, read_import_entries, read_import_report};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_transcript_import_cutover<F>(
    source_database_path: &std::path::Path,
    transcript_root: &std::path::Path,
    target_path: &std::path::Path,
    import_id: CommandId,
    committed_at_ms: i64,
    before_final_source_check: F,
) -> Result<crate::TranscriptImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let result = commit_transcript_import_cutover_once(
        source_database_path,
        transcript_root,
        target_path,
        import_id,
        committed_at_ms,
        before_final_source_check,
    );
    if matches!(result, Err(StorageError::SourceChanged)) {
        let _ = commit_transcript_import_cutover_once(
            source_database_path,
            transcript_root,
            target_path,
            import_id,
            committed_at_ms,
            || Ok(()),
        );
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn commit_transcript_import_cutover_once<F>(
    source_database_path: &std::path::Path,
    transcript_root: &std::path::Path,
    target_path: &std::path::Path,
    import_id: CommandId,
    committed_at_ms: i64,
    before_final_source_check: F,
) -> Result<crate::TranscriptImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    if committed_at_ms < 0 {
        return Err(StorageError::TranscriptImportConflict);
    }
    let source = inspect_transcript_source(source_database_path, transcript_root)?;
    let mut final_check = Some(before_final_source_check);
    let receipt = TransitionCommit::open(target_path)?.commit_planned_with_transaction_hooks(
        ingress(import_id),
        UnixTimestampMilliseconds::new(committed_at_ms),
        |transaction| {
            let current = report(transaction, import_id)?;
            let current_revision = core_revision(transaction)?;
            let reserved = reserved_revision_matches(transaction, current.target_revision.value)?;
            let (disposition, discard) = match current.state {
                TranscriptImportState::Verified if current.plan == source.plan && reserved => {
                    (RequestDisposition::Accepted, None)
                }
                TranscriptImportState::Committed => (RequestDisposition::AlreadyComplete, None),
                TranscriptImportState::Verified => (
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::RevisionConflict,
                    },
                    Some(if current.plan == source.plan {
                        StorageError::TranscriptImportConflict.code()
                    } else {
                        StorageError::SourceChanged.code()
                    }),
                ),
                _ => (
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::MissingPrerequisite,
                    },
                    None,
                ),
            };
            plan_transcript_cutover(TranscriptCutoverActivityInput {
                command_id: import_id,
                current_revision,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    StateRevision::new(
                        current_revision
                            .value
                            .checked_add(1)
                            .ok_or(StorageError::InvalidActivity)?,
                    )
                } else {
                    current_revision
                },
                disposition,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, current, discard)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |_| Ok(()),
        |transaction, expected, (mutation, current, discard)| match mutation {
            TranscriptCutoverMutation::Apply => {
                require_core_revision(transaction, expected)?;
                let entries = read_import_entries(transaction, import_id)?;
                crate::transcript_import_commit::apply_verified_transcript_import(
                    transaction,
                    import_id,
                    committed_at_ms,
                    &current,
                    &entries,
                )
            }
            TranscriptCutoverMutation::None => {
                require_core_revision(transaction, expected)?;
                if let Some(diagnostic) = discard {
                    crate::transcript_import_discard::discard_transcript_import_in_transaction(
                        transaction,
                        import_id,
                        current.target_revision.value,
                        committed_at_ms,
                        diagnostic,
                    )?;
                }
                Ok(expected)
            }
        },
        |_| {
            if let Some(check) = final_check.take() {
                check()?;
            }
            if inspect_transcript_source(source_database_path, transcript_root)?.plan != source.plan
            {
                return Err(StorageError::SourceChanged);
            }
            Ok(())
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted
        | RequestDisposition::AlreadyComplete
        | RequestDisposition::Duplicate => {
            let connection = open_current(target_path)?;
            read_import_report(&connection, import_id, true)?
                .ok_or(StorageError::TranscriptImportNotFound)
        }
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        } => {
            let current = {
                let connection = open_current(target_path)?;
                read_import_report(&connection, import_id, true)?
                    .ok_or(StorageError::TranscriptImportNotFound)?
            };
            if current.diagnostic_code.as_deref() == Some(StorageError::SourceChanged.code()) {
                Err(StorageError::SourceChanged)
            } else {
                Err(StorageError::TranscriptImportConflict)
            }
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::TranscriptImportConflict),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn reserved_revision_matches(
    connection: &rusqlite::Connection,
    target: u64,
) -> Result<bool, StorageError> {
    let current: i64 = connection
        .query_row(
            "SELECT collection_revision FROM pod0_transcript_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read transcript collection revision", error))?;
    Ok(u64::try_from(current).ok() == target.checked_sub(1))
}

fn report(
    connection: &rusqlite::Connection,
    import_id: CommandId,
) -> Result<crate::TranscriptImportReport, StorageError> {
    read_import_report(connection, import_id, true)?.ok_or(StorageError::TranscriptImportNotFound)
}

fn ingress(import_id: CommandId) -> TransitionIngress {
    let mut hash = Sha256::new();
    hash.update(b"pod0-transcript-import-cutover-v1");
    hash.update(import_id.into_bytes());
    TransitionIngress {
        kind: TransitionIngressKind::Migration,
        id: import_id.into_bytes(),
        fingerprint: ContentDigest::from_bytes(hash.finalize().into()),
    }
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read transcript cutover revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::TranscriptImportConflict)
}
