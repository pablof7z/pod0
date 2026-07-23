use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_signer_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_signer_state",
        &[
            "account_id",
            "credential_kind_code",
            "credential_kind_wire_code",
            "expected_author_hex",
            "safe_detail",
            "singleton",
            "stage_code",
            "state_revision",
            "updated_at_ms",
        ],
    )
}
