use std::sync::atomic::{AtomicBool, Ordering};

use pod0_domain::CommandId;

use crate::migration::{MigrationBoundary, MigrationObserver};
use crate::{CoreStoreMigrator, MigrationClock, StorageError};

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

struct InterruptBeforeRetirement {
    triggered: AtomicBool,
}

impl MigrationObserver for InterruptBeforeRetirement {
    fn reached(&self, boundary: MigrationBoundary) -> Result<(), StorageError> {
        if boundary == (MigrationBoundary::BeforeStepCommit { target: 44 })
            && !self.triggered.swap(true, Ordering::SeqCst)
        {
            Err(StorageError::Interrupted)
        } else {
            Ok(())
        }
    }
}

#[test]
fn removal_migration_clears_external_publication_state_without_touching_unrelated_data() {
    let fixture = Fixture::at_v43();
    seed_all_states(&fixture.store);

    fixture.migrate().unwrap();

    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    assert_eq!(table_count(&connection, "pod0_publications"), 0);
    assert_eq!(table_count(&connection, "pod0_publication_facts"), 0);
    assert_eq!(table_count(&connection, "pod0_publication_commands"), 0);
    assert_eq!(table_count(&connection, "pod0_signer_state"), 0);
    assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM pod0_effect_intents WHERE effect_kind_code=14"), 0);
    assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM pod0_activity_facts WHERE subject_code=7"), 0);
    assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM pod0_activity_facts WHERE subject_code=0"), 1);
    assert_eq!(scalar(&connection, "SELECT authority_active FROM pod0_memory_state WHERE singleton=1"), 1);
}

#[test]
fn removal_is_atomic_across_interruption_and_idempotent_on_restart() {
    let fixture = Fixture::at_v43();
    seed_all_states(&fixture.store);
    let observer = InterruptBeforeRetirement {
        triggered: AtomicBool::new(false),
    };

    let error = fixture
        .migrator
        .migrate_with_observer(
            &fixture.store,
            44,
            &fixture.backup,
            CommandId::from_parts(0, 44),
            &observer,
        )
        .unwrap_err();
    assert_eq!(error, StorageError::Interrupted);

    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    assert_eq!(table_count(&connection, "pod0_publications"), 1);
    assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM pod0_publications"), 5);
    drop(connection);

    fixture.migrate().unwrap();
    let report = fixture.migrate().unwrap();
    assert!(report.applied_versions.is_empty());
    assert_eq!(report.to_version, 44);
}

struct Fixture {
    _directory: tempfile::TempDir,
    store: std::path::PathBuf,
    backup: std::path::PathBuf,
    migrator: CoreStoreMigrator<FixedClock>,
}

impl Fixture {
    fn at_v43() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("core.sqlite");
        let backup = directory.path().join("core.backup.sqlite");
        let migrator = CoreStoreMigrator::new(FixedClock);
        migrator
            .migrate(&store, 43, &backup, CommandId::from_parts(0, 43))
            .unwrap();
        Self {
            _directory: directory,
            store,
            backup,
            migrator,
        }
    }

    fn migrate(&self) -> Result<crate::MigrationReport, StorageError> {
        self.migrator.migrate(
            &self.store,
            44,
            &self.backup,
            CommandId::from_parts(0, 44),
        )
    }
}

fn seed_all_states(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.execute_batch(SEED_SQL).unwrap();
}

fn scalar(connection: &rusqlite::Connection, sql: &str) -> i64 {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn table_count(connection: &rusqlite::Connection, table: &str) -> i64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        )
        .unwrap()
}

const SEED_SQL: &str = r#"
UPDATE pod0_signer_state SET
 account_id=X'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',credential_kind_code='generated',
 expected_author_hex=printf('%064d',0),state_revision=1,stage_code='ready',updated_at_ms=1;

INSERT INTO pod0_activity_facts(
 activity_id,transaction_id,correlation_id,authorized_effect_intent_id,
 actor_code,origin_code,subject_code,subject_id,fact_code,payload_json,committed_at_ms
) VALUES(
 X'01010101010101010101010101010101',X'02020202020202020202020202020202',
 X'03030303030303030303030303030303',X'11111111111111111111111111111111',
 2,2,7,X'10101010101010101010101010101010',4,'{}',1
);
INSERT INTO pod0_activity_facts(
 activity_id,transaction_id,correlation_id,actor_code,origin_code,subject_code,
 fact_code,payload_json,committed_at_ms
) VALUES(
 X'04040404040404040404040404040404',X'05050505050505050505050505050505',
 X'06060606060606060606060606060606',2,2,0,1,'{}',1
);
INSERT INTO pod0_effect_intents(
 intent_id,authorizing_activity_id,correlation_id,effect_kind_code,subject_code,subject_id,
 request_json,state_code,fence,available_at_ms,committed_at_ms
) VALUES(
 X'11111111111111111111111111111111',X'01010101010101010101010101010101',
 X'03030303030303030303030303030303',14,7,X'10101010101010101010101010101010',
 '{}',2,1,1,1
);
INSERT INTO pod0_effect_attempts(
 attempt_id,intent_id,lease_id,fence,state_code,claimed_at_ms,lease_expires_at_ms,
 observed_at_ms,outcome_schema_version,outcome_json,observation_schema_version,observation_json
) VALUES(
 X'12121212121212121212121212121212',X'11111111111111111111111111111111',
 X'13131313131313131313131313131313',1,2,1,100,2,1,'{}',1,'{}'
);

INSERT INTO pod0_publications(
 publication_id,artifact_id,artifact_kind_code,episode_id,podcast_id,semantic_revision,
 state_revision,expected_author_hex,correlation_token,public_media_url,media_type,
 media_byte_count,media_content_digest,receipt_id,event_id_hex,stage_code,prepared_at_ms,updated_at_ms
) VALUES
 (X'10101010101010101010101010101010',X'20202020202020202020202020202020',1,X'30303030303030303030303030303030',X'40404040404040404040404040404040',1,1,printf('%064d',0),'pending','https://example.com/1.mp3','audio/mpeg',1,zeroblob(32),NULL,NULL,'pending',1,1),
 (X'11101010101010101010101010101010',X'21202020202020202020202020202020',1,X'31303030303030303030303030303030',X'41404040404040404040404040404040',1,1,printf('%064d',0),'leased','https://example.com/2.mp3','audio/mpeg',1,zeroblob(32),NULL,NULL,'leased',1,1),
 (X'12101010101010101010101010101010',X'22202020202020202020202020202020',1,X'32303030303030303030303030303030',X'42404040404040404040404040404040',1,1,printf('%064d',0),'retryable','https://example.com/3.mp3','audio/mpeg',1,zeroblob(32),NULL,NULL,'retryable',1,1),
 (X'13101010101010101010101010101010',X'23202020202020202020202020202020',1,X'33303030303030303030303030303030',X'43404040404040404040404040404040',1,1,printf('%064d',0),'completed','https://example.com/4.mp3','audio/mpeg',1,zeroblob(32),X'0101010101010101',printf('%064d',1),'completed',1,1),
 (X'14101010101010101010101010101010',X'24202020202020202020202020202020',1,X'34303030303030303030303030303030',X'44404040404040404040404040404040',1,1,printf('%064d',0),'ambiguous','https://example.com/5.mp3','audio/mpeg',1,zeroblob(32),X'0202020202020202',NULL,'retryable',1,1);
INSERT INTO pod0_publication_facts(
 publication_id,sequence_number,fact_digest,fact_kind_code,observed_at_ms
) VALUES(X'10101010101010101010101010101010',1,zeroblob(32),'accepted',1);
INSERT INTO pod0_publication_commands(
 command_id,command_fingerprint,publication_id,completed_at_ms
) VALUES(X'50505050505050505050505050505050',printf('%064d',0),X'10101010101010101010101010101010',1);
"#;
