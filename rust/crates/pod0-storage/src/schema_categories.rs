use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_category_schema(
    connection: &Connection,
    version: u32,
) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_category_state",
        &["collection_revision", "singleton"],
    )?;
    require_columns(
        connection,
        "pod0_categories",
        &[
            "category_id",
            "category_revision",
            "color_hex",
            "created_at_ms",
            "created_command_id",
            "deleted",
            "description",
            "name",
            "origin_code",
            "slug",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_category_members",
        &["added_at_ms", "category_id", "item_id", "item_kind_code"],
    )?;
    if version >= 47 {
        require_columns(
            connection,
            "pod0_category_settings",
            &[
                "auto_download_code",
                "auto_download_latest_count",
                "category_id",
                "notifications_enabled",
                "rag_enabled",
                "updated_at_ms",
                "wifi_only",
            ],
        )?;
    }
    Ok(())
}
