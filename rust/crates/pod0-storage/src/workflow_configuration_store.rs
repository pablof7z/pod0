use pod0_application::{
    WorkflowCapabilitySnapshot, WorkflowConfiguration, WorkflowConfigurationInput,
    WorkflowConfigurationOrigin,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::{CommitReceipt, LibraryStore, StorageError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowConfigurationCommitOutcome {
    pub configuration: Option<WorkflowConfiguration>,
    pub changed: bool,
    pub imported: bool,
    pub receipt: CommitReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowCapabilityCommitOutcome {
    pub snapshot: Option<WorkflowCapabilitySnapshot>,
    pub changed: bool,
    pub receipt: CommitReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowReconcileCommitOutcome {
    pub receipt: CommitReceipt,
    pub authorized_command_count: u16,
}

impl LibraryStore {
    pub fn workflow_configuration(&self) -> Result<Option<WorkflowConfiguration>, StorageError> {
        self.read(read_configuration)
    }

    pub fn workflow_capability_snapshot(
        &self,
    ) -> Result<Option<WorkflowCapabilitySnapshot>, StorageError> {
        self.read(read_capability_snapshot)
    }

    pub fn import_legacy_workflow_configuration(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        input: WorkflowConfigurationInput,
        source_generation: ContentDigest,
        observed_at_ms: i64,
    ) -> Result<WorkflowConfigurationCommitOutcome, StorageError> {
        crate::transition_commit::commit_workflow_configuration_import(
            self.path(),
            command_id,
            fingerprint,
            input,
            source_generation,
            observed_at_ms,
        )
    }

    pub fn set_workflow_configuration(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        expected_revision: StateRevision,
        input: WorkflowConfigurationInput,
        observed_at_ms: i64,
    ) -> Result<WorkflowConfigurationCommitOutcome, StorageError> {
        crate::transition_commit::commit_workflow_configuration_set(
            self.path(),
            command_id,
            fingerprint,
            expected_revision,
            input,
            observed_at_ms,
        )
    }

    pub fn observe_workflow_capabilities(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        snapshot: WorkflowCapabilitySnapshot,
    ) -> Result<WorkflowCapabilityCommitOutcome, StorageError> {
        crate::transition_commit::commit_workflow_capabilities(
            self.path(),
            command_id,
            fingerprint,
            snapshot,
        )
    }

    pub fn reconcile_workflow_opportunity(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        opportunity: pod0_application::WorkflowOpportunity,
    ) -> Result<crate::WorkflowReconcileCommitOutcome, StorageError> {
        crate::transition_commit::commit_workflow_reconcile(
            self.path(),
            command_id,
            fingerprint,
            opportunity,
            0,
        )
    }

    pub fn continue_workflow_reconciliation_from_internal_command(
        &self,
        command: crate::PendingInternalCommand,
    ) -> Result<crate::WorkflowReconcileCommitOutcome, StorageError> {
        crate::transition_commit::commit_workflow_reconcile_from_internal_command(
            self.path(),
            command,
        )
    }
}

pub(crate) fn read_configuration(
    connection: &Connection,
) -> Result<Option<WorkflowConfiguration>, StorageError> {
    connection
        .query_row(
            "SELECT schema_version,revision,origin,configuration_json \
             FROM pod0_workflow_configuration WHERE singleton=1 AND authority_state='authoritative'",
            [],
            decode_configuration,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read workflow configuration", error))?
        .map(validate_configuration)
        .transpose()
}

pub(crate) fn read_capability_snapshot(
    connection: &Connection,
) -> Result<Option<WorkflowCapabilitySnapshot>, StorageError> {
    connection
        .query_row(
            "SELECT snapshot_json FROM pod0_workflow_capability_snapshot WHERE singleton=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read workflow capabilities", error))?
        .map(|json| serde_json::from_str(&json).map_err(|_| StorageError::InvalidActivity))
        .transpose()
}

pub(crate) fn write_configuration(
    transaction: &Transaction<'_>,
    value: &WorkflowConfiguration,
    source_generation: Option<ContentDigest>,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let origin = match value.origin {
        WorkflowConfigurationOrigin::LegacySwiftImport => "legacy_swift_import",
        WorkflowConfigurationOrigin::User => "user",
    };
    let json = serde_json::to_string(&value.value).map_err(|_| StorageError::InvalidActivity)?;
    let revision =
        i64::try_from(value.revision.value).map_err(|_| StorageError::InvalidActivity)?;
    transaction
        .execute(
            "INSERT INTO pod0_workflow_configuration(singleton,schema_version,authority_state,origin,\
             revision,configuration_json,source_generation,created_at_ms,updated_at_ms) \
             VALUES(1,?1,'authoritative',?2,?3,?4,?5,?6,?6) \
             ON CONFLICT(singleton) DO UPDATE SET schema_version=excluded.schema_version,\
             origin=excluded.origin,revision=excluded.revision,configuration_json=excluded.configuration_json,\
             source_generation=excluded.source_generation,updated_at_ms=excluded.updated_at_ms",
            params![value.schema_version, origin, revision, json,
                source_generation.map(ContentDigest::into_bytes).as_ref().map(<[u8; 32]>::as_slice),
                observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("write workflow configuration", error))?;
    Ok(())
}

pub(crate) fn write_capability_snapshot(
    transaction: &Transaction<'_>,
    snapshot: &WorkflowCapabilitySnapshot,
    revision: StateRevision,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(snapshot).map_err(|_| StorageError::InvalidActivity)?;
    let revision = i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?;
    transaction.execute(
        "INSERT INTO pod0_workflow_capability_snapshot(singleton,schema_version,snapshot_id,revision,\
         snapshot_json,observed_at_ms) VALUES(1,1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET \
         snapshot_id=excluded.snapshot_id,revision=excluded.revision,snapshot_json=excluded.snapshot_json,\
         observed_at_ms=excluded.observed_at_ms",
        params![snapshot.snapshot_id.into_bytes().as_slice(), revision, json,
            snapshot.observed_at.value],
    ).map_err(|error| StorageError::sqlite("write workflow capabilities", error))?;
    Ok(())
}

fn decode_configuration(row: &Row<'_>) -> rusqlite::Result<StoredConfiguration> {
    Ok(StoredConfiguration {
        schema_version: row.get(0)?,
        revision: row.get(1)?,
        origin: row.get(2)?,
        json: row.get(3)?,
    })
}

struct StoredConfiguration {
    schema_version: i64,
    revision: i64,
    origin: String,
    json: String,
}

fn validate_configuration(
    stored: StoredConfiguration,
) -> Result<WorkflowConfiguration, StorageError> {
    let origin = match stored.origin.as_str() {
        "legacy_swift_import" => WorkflowConfigurationOrigin::LegacySwiftImport,
        "user" => WorkflowConfigurationOrigin::User,
        _ => return Err(StorageError::InvalidActivity),
    };
    let value: WorkflowConfigurationInput =
        serde_json::from_str(&stored.json).map_err(|_| StorageError::InvalidActivity)?;
    if stored.schema_version != i64::from(pod0_application::WORKFLOW_CONFIGURATION_SCHEMA_VERSION)
        || stored.revision < 0
        || value.validate().is_err()
    {
        return Err(StorageError::InvalidActivity);
    }
    Ok(WorkflowConfiguration {
        schema_version: u32::try_from(stored.schema_version)
            .map_err(|_| StorageError::InvalidActivity)?,
        revision: StateRevision::new(
            u64::try_from(stored.revision).map_err(|_| StorageError::InvalidActivity)?,
        ),
        origin,
        value,
    })
}
