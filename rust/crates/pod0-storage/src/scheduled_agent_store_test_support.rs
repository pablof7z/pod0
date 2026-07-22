use pod0_application::{ScheduledTaskDefinition, scheduled_prompt_revision};
use pod0_domain::{
    CancellationId, CommandId, ScheduledTaskId, StateRevision, UnixTimestampMilliseconds,
};
use rusqlite::Connection;
use tempfile::TempDir;

use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock, ScheduledAgentCommandContext,
    ScheduledAgentStore,
};

pub(crate) struct ScheduledFixture {
    pub(crate) _directory: TempDir,
    pub(crate) path: std::path::PathBuf,
    pub(crate) store: ScheduledAgentStore,
}

impl ScheduledFixture {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("core.sqlite");
        let backup = directory.path().join("core.backup.sqlite");
        CoreStoreMigrator::new(FixedClock)
            .migrate(
                &path,
                CURRENT_SCHEMA_VERSION,
                &backup,
                CommandId::from_parts(900, 1),
            )
            .unwrap();
        activate(&path);
        let store = ScheduledAgentStore::open_authoritative(&path).unwrap();
        Self {
            _directory: directory,
            path,
            store,
        }
    }

    pub(crate) fn context(&self, value: u64, at_ms: i64) -> ScheduledAgentCommandContext {
        ScheduledAgentCommandContext {
            command_id: CommandId::from_parts(901, value),
            command_fingerprint: [u8::try_from(value).unwrap_or(255); 32],
            cancellation_id: CancellationId::from_parts(902, value),
            issued_revision: StateRevision::new(value),
            observed_at: time(at_ms),
        }
    }

    pub(crate) fn definition(&self, value: u64, due_ms: i64) -> ScheduledTaskDefinition {
        let prompt = format!("Prepare briefing {value}");
        ScheduledTaskDefinition {
            task_id: ScheduledTaskId::from_parts(903, value),
            label: format!("Briefing {value}"),
            prompt_revision: scheduled_prompt_revision(&prompt).unwrap(),
            prompt,
            model_reference: "openrouter:test/model".to_owned(),
            interval_milliseconds: 86_400_000,
            created_at: time(1_000 + i64::try_from(value).unwrap()),
            last_run_at: None,
            next_run_at: time(due_ms),
            revision: StateRevision::new(1),
        }
    }
}

pub(crate) struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_900_000_000_000
    }
}

pub(crate) fn time(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}

pub(crate) fn activate(path: &std::path::Path) {
    Connection::open(path)
        .unwrap()
        .execute(
            "UPDATE pod0_scheduled_agent_authority SET state='authoritative',source_generation=1,\
         committed_at_ms=1 WHERE singleton=1 AND state='inactive'",
            [],
        )
        .unwrap();
}
