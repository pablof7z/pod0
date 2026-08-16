use pod0_application::{
    ChapterCutoverActivityInput, ChapterCutoverMutation, RequestDisposition,
    RequestRejectionReason, plan_chapter_cutover,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::chapter_import_store_read::{open_current, read_import_report};
use crate::legacy_chapter_source::inspect_chapter_source;
use crate::{ChapterImportState, StorageError, TransitionIngress, TransitionIngressKind};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_chapter_import_cutover<F>(
    source_database_path: &std::path::Path,
    artifact_root: &std::path::Path,
    target_path: &std::path::Path,
    import_id: CommandId,
    imported_at_ms: i64,
    before_final_source_check: F,
) -> Result<crate::ChapterImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let result = commit_once(
        source_database_path,
        artifact_root,
        target_path,
        import_id,
        imported_at_ms,
        before_final_source_check,
    );
    if matches!(result, Err(StorageError::SourceChanged)) {
        let _ = commit_once(
            source_database_path,
            artifact_root,
            target_path,
            import_id,
            imported_at_ms,
            || Ok(()),
        );
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn commit_once<F>(
    source_database_path: &std::path::Path,
    artifact_root: &std::path::Path,
    target_path: &std::path::Path,
    import_id: CommandId,
    imported_at_ms: i64,
    before_final_source_check: F,
) -> Result<crate::ChapterImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    if imported_at_ms < 0 {
        return Err(StorageError::ChapterImportConflict);
    }
    let source = inspect_chapter_source(source_database_path, artifact_root)?;
    let mut final_check = Some(before_final_source_check);
    let receipt = TransitionCommit::open(target_path)?.commit_planned_with_transaction_hooks(
        ingress(import_id),
        UnixTimestampMilliseconds::new(imported_at_ms),
        |transaction| {
            let report = report(transaction, import_id)?;
            let current_revision = core_revision(transaction)?;
            let authority = crate::chapter_import_commit::authority_state(transaction)?;
            let valid = matches!(
                report.state,
                ChapterImportState::Verified | ChapterImportState::Imported
            )
                && report.plan.blocked_count == 0
                && report.plan == source.plan
                && authority == (false, None);
            let disposition = if valid {
                RequestDisposition::Accepted
            } else if report.state == ChapterImportState::Imported
                && authority == (true, Some(import_id.into_bytes()))
            {
                RequestDisposition::AlreadyComplete
            } else if report.state == ChapterImportState::Verified {
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::RevisionConflict,
                }
            } else {
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingPrerequisite,
                }
            };
            plan_chapter_cutover(ChapterCutoverActivityInput {
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
            .map(|plan| plan.map_mutation(|mutation| (mutation, report)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |_| Ok(()),
        |transaction, expected, (mutation, report)| match mutation {
            ChapterCutoverMutation::Apply => {
                require_core_revision(transaction, expected)?;
                crate::chapter_import_commit::apply_verified_chapter_import(
                    transaction,
                    import_id,
                    imported_at_ms,
                    &report,
                    &source,
                    expected,
                )
            }
            ChapterCutoverMutation::None => {
                require_core_revision(transaction, expected)?;
                if report.state == ChapterImportState::Verified && report.plan != source.plan {
                    crate::chapter_import_verification::mark_corrupt_in_transaction(
                        transaction,
                        import_id,
                        StorageError::SourceChanged.code(),
                    )?;
                }
                Ok(expected)
            }
        },
        |_| {
            if let Some(check) = final_check.take() {
                check()?;
            }
            if inspect_chapter_source(source_database_path, artifact_root)? != source {
                return Err(StorageError::SourceChanged);
            }
            Ok(())
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::AlreadyComplete => {
            let connection = open_current(target_path)?;
            report(&connection, import_id)
        }
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        } => {
            let connection = open_current(target_path)?;
            let current = report(&connection, import_id)?;
            if current.diagnostic_code.as_deref() == Some(StorageError::SourceChanged.code()) {
                Err(StorageError::SourceChanged)
            } else {
                Err(StorageError::ChapterImportConflict)
            }
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::ChapterImportConflict),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn report(
    connection: &rusqlite::Connection,
    import_id: CommandId,
) -> Result<crate::ChapterImportReport, StorageError> {
    read_import_report(connection, import_id, true)?.ok_or(StorageError::ChapterImportNotFound)
}

fn ingress(import_id: CommandId) -> TransitionIngress {
    let mut hash = Sha256::new();
    hash.update(b"pod0-chapter-import-cutover-v1");
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
            "SELECT collection_revision FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read chapter cutover revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::ChapterImportConflict)
}
