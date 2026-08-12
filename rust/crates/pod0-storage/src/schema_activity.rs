use rusqlite::{Connection, OptionalExtension};

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_activity_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_activity_facts",
        &[
            "activity_id",
            "actor_code",
            "authorized_effect_intent_id",
            "authorized_internal_command_id",
            "caused_by_activity_id",
            "command_id",
            "committed_at_ms",
            "correlation_id",
            "episode_id",
            "fact_code",
            "host_request_id",
            "origin_code",
            "payload_json",
            "payload_schema_version",
            "sequence",
            "subject_code",
            "subject_id",
            "transaction_id",
        ],
    )?;
    require_columns(
        connection,
        "pod0_transition_receipts",
        &[
            "committed_at_ms",
            "committed_revision",
            "disposition_code",
            "fingerprint",
            "first_sequence",
            "ingress_code",
            "ingress_id",
            "last_sequence",
            "result_json",
            "result_schema_version",
            "transaction_id",
        ],
    )?;
    require_columns(
        connection,
        "pod0_effect_intents",
        &[
            "authorizing_activity_id",
            "authorizing_fact_code",
            "available_at_ms",
            "committed_at_ms",
            "correlation_id",
            "deadline_at_ms",
            "effect_kind_code",
            "episode_id",
            "fence",
            "intent_id",
            "request_json",
            "request_schema_version",
            "state_code",
            "subject_code",
            "subject_id",
        ],
    )?;
    require_columns(
        connection,
        "pod0_effect_attempts",
        &[
            "attempt_id",
            "claimed_at_ms",
            "fence",
            "intent_id",
            "lease_expires_at_ms",
            "lease_id",
            "observed_at_ms",
            "observation_json",
            "observation_schema_version",
            "outcome_json",
            "outcome_schema_version",
            "state_code",
        ],
    )?;
    require_columns(
        connection,
        "pod0_internal_command_intents",
        &[
            "authorizing_activity_id",
            "authorizing_fact_code",
            "command_json",
            "command_schema_version",
            "committed_at_ms",
            "correlation_id",
            "episode_id",
            "internal_command_id",
            "state_code",
            "subject_code",
            "subject_id",
            "target_domain_code",
        ],
    )?;
    for trigger in [
        "pod0_activity_facts_no_update",
        "pod0_activity_facts_no_delete",
    ] {
        let found: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='trigger' AND name=?1",
                [trigger],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| {
                StorageError::sqlite("validate activity append-only trigger", error)
            })?;
        if found.is_none() {
            return Err(StorageError::CorruptSchema {
                detail: "activity append-only trigger is missing",
            });
        }
    }
    Ok(())
}
