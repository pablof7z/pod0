use pod0_application::RequestRejectionReason;
use pod0_domain::{
    CategoryId, CategoryRevision, CategorySettings, CommandId, StateRevision,
    validate_category_settings,
};

use crate::StorageError;

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_category_settings(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    category_id: CategoryId,
    expected_revision: CategoryRevision,
    settings: CategorySettings,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    super::category::commit(
        path,
        command_id,
        command_fingerprint,
        observed_at_ms,
        |transaction| {
            let rejection =
                match crate::category_store_read::category_revision(transaction, category_id)? {
                    None => Some(RequestRejectionReason::MissingSubject),
                    Some(actual) if actual != expected_revision => {
                        Some(RequestRejectionReason::RevisionConflict)
                    }
                    Some(_) if validate_category_settings(settings).is_err() => {
                        Some(RequestRejectionReason::Invalid)
                    }
                    Some(_) => None,
                };
            Ok(rejection)
        },
        |transaction| {
            crate::library_store_categories::policy::update_settings_in_transaction(
                transaction,
                command_id,
                command_fingerprint,
                category_id,
                settings,
                observed_at_ms,
            )
        },
    )
}
