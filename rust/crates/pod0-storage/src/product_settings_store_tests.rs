use pod0_application::{RequestDisposition, SettingsConflictWinner, SettingsValidationState};
use pod0_domain::{
    CommandId, ContentDigest, PRODUCT_SETTINGS_SCHEMA_VERSION, ProductSettingsValues,
    SettingsWriterVersion,
};
use rusqlite::Connection;

use crate::library_store_tests::imported_fixture;
use crate::{LibraryStore, commit_listening_cutover};

#[test]
fn clean_legacy_import_is_durable_authoritative_and_restart_safe() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(store.product_settings().unwrap(), None);

    let outcome = store
        .import_legacy_product_settings(
            command(1),
            digest(1),
            7,
            digest(8),
            ProductSettingsValues::default(),
            1_800_000_000_001,
        )
        .unwrap();
    assert!(outcome.changed);
    assert!(outcome.authoritative);
    assert_eq!(outcome.validation, SettingsValidationState::Valid);
    let settings = outcome.settings.unwrap();
    assert_eq!(settings.schema_version, PRODUCT_SETTINGS_SCHEMA_VERSION);
    assert_eq!(settings.values, ProductSettingsValues::default());
    assert_eq!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .product_settings()
            .unwrap(),
        Some(settings)
    );
}

#[test]
fn populated_legacy_import_preserves_values_and_single_writer_authority() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let values = ProductSettingsValues {
        agent_display_name: "Migrated Agent".to_owned(),
        skip_forward_seconds: 45,
        ..ProductSettingsValues::default()
    };

    let outcome = store
        .import_legacy_product_settings(
            command(2),
            digest(2),
            41,
            digest(8),
            values.clone(),
            1_800_000_000_002,
        )
        .unwrap();

    assert!(outcome.authoritative);
    assert_eq!(outcome.settings.unwrap().values, values);
    let replay = store
        .import_legacy_product_settings(
            command(3),
            digest(3),
            42,
            digest(9),
            ProductSettingsValues::default(),
            1_800_000_000_003,
        )
        .unwrap();
    assert_eq!(
        replay.receipt.disposition,
        RequestDisposition::AlreadyComplete
    );
    assert!(!replay.changed);
    assert!(replay.authoritative);
    assert_eq!(replay.settings.unwrap().values, values);
}

#[test]
fn interrupted_legacy_import_rolls_back_and_retries_atomically() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let connection = Connection::open(&fixture.target).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_settings_authority BEFORE INSERT ON pod0_domain_cutovers \
             WHEN NEW.domain='product_settings' BEGIN SELECT RAISE(ABORT,'injected'); END;",
        )
        .unwrap();

    assert!(
        store
            .import_legacy_product_settings(
                command(4),
                digest(4),
                5,
                digest(8),
                ProductSettingsValues::default(),
                1_800_000_000_004,
            )
            .is_err()
    );
    assert_eq!(store.product_settings().unwrap(), None);
    assert!(!store.product_settings_is_authoritative().unwrap());

    connection
        .execute_batch("DROP TRIGGER fail_settings_authority;")
        .unwrap();
    let retry = store
        .import_legacy_product_settings(
            command(4),
            digest(4),
            5,
            digest(8),
            ProductSettingsValues::default(),
            1_800_000_000_004,
        )
        .unwrap();
    assert!(retry.changed);
    assert!(retry.authoritative);
}

#[test]
fn unmarked_existing_settings_fail_closed_without_overwrite() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let original = ProductSettingsValues {
        agent_display_name: "Original".to_owned(),
        ..ProductSettingsValues::default()
    };
    store
        .import_legacy_product_settings(
            command(5),
            digest(5),
            1,
            digest(8),
            original.clone(),
            1_800_000_000_005,
        )
        .unwrap();
    Connection::open(&fixture.target)
        .unwrap()
        .execute(
            "DELETE FROM pod0_domain_cutovers WHERE domain='product_settings'",
            [],
        )
        .unwrap();

    let result = store.import_legacy_product_settings(
        command(6),
        digest(6),
        2,
        digest(9),
        ProductSettingsValues::default(),
        1_800_000_000_006,
    );

    assert!(matches!(result, Err(crate::StorageError::ImportConflict)));
    assert_eq!(store.product_settings().unwrap().unwrap().values, original);
    assert!(!store.product_settings_is_authoritative().unwrap());
}

#[test]
fn equal_counter_remote_conflicts_converge_and_preserve_evidence() {
    let left_fixture = imported_fixture();
    let right_fixture = imported_fixture();
    commit_listening_cutover(&left_fixture.target, 1_800_000_000_000).unwrap();
    commit_listening_cutover(&right_fixture.target, 1_800_000_000_000).unwrap();
    let left = LibraryStore::open_authoritative(&left_fixture.target).unwrap();
    let right = LibraryStore::open_authoritative(&right_fixture.target).unwrap();
    seed(&left, 10, 1);
    seed(&right, 20, 2);

    let left_state = left.product_settings().unwrap().unwrap();
    let right_state = right.product_settings().unwrap().unwrap();
    let left_merge = left
        .merge_remote_product_settings(
            command(30),
            digest(30),
            right_state.schema_version,
            right_state.writer_version,
            right_state.values.clone(),
            1_800_000_000_030,
        )
        .unwrap();
    let right_merge = right
        .merge_remote_product_settings(
            command(31),
            digest(31),
            left_state.schema_version,
            left_state.writer_version,
            left_state.values,
            1_800_000_000_031,
        )
        .unwrap();

    assert_eq!(
        left_merge.settings.as_ref().unwrap().values,
        right_merge.settings.as_ref().unwrap().values
    );
    assert_eq!(
        left_merge.conflict.unwrap().winner,
        SettingsConflictWinner::Candidate
    );
    assert_eq!(
        right_merge.conflict.unwrap().winner,
        SettingsConflictWinner::Current
    );
    let reopened = LibraryStore::open_authoritative(&left_fixture.target).unwrap();
    assert_eq!(reopened.product_settings().unwrap(), left_merge.settings);
    let connection = Connection::open(&left_fixture.target).unwrap();
    assert!(
        connection
            .execute(
                "UPDATE pod0_settings_validation_evidence SET observed_at_ms=observed_at_ms",
                [],
            )
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM pod0_settings_sync_conflicts", [])
            .is_err()
    );
}

#[test]
fn invalid_remote_value_records_validation_without_mutating_settings() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    seed(&store, 40, 1);
    let before = store.product_settings().unwrap().unwrap();
    let mut invalid = before.values.clone();
    invalid.skip_forward_seconds = 0;

    let outcome = store
        .merge_remote_product_settings(
            command(42),
            digest(42),
            PRODUCT_SETTINGS_SCHEMA_VERSION,
            SettingsWriterVersion {
                counter: 99,
                writer_id: digest(9),
            },
            invalid.clone(),
            1_800_000_000_042,
        )
        .unwrap();
    assert!(matches!(
        outcome.receipt.disposition,
        RequestDisposition::Rejected { .. }
    ));
    assert_ne!(outcome.validation, SettingsValidationState::Valid);
    assert_eq!(outcome.settings, Some(before));

    let replay = LibraryStore::open_authoritative(&fixture.target)
        .unwrap()
        .merge_remote_product_settings(
            command(42),
            digest(42),
            PRODUCT_SETTINGS_SCHEMA_VERSION,
            SettingsWriterVersion {
                counter: 99,
                writer_id: digest(9),
            },
            invalid,
            1_800_000_000_042,
        )
        .unwrap();
    assert!(replay.receipt.replayed);
    assert_eq!(replay.validation, outcome.validation);
}

fn seed(store: &LibraryStore, command_offset: u64, writer: u8) {
    let defaults = store
        .import_legacy_product_settings(
            command(command_offset),
            digest(command_offset as u8),
            command_offset,
            digest(writer),
            ProductSettingsValues::default(),
            1_800_000_000_000 + command_offset as i64,
        )
        .unwrap()
        .settings
        .unwrap();
    let mut values = defaults.values;
    values.agent_display_name = format!("device-{writer}");
    store
        .set_local_product_settings(
            command(command_offset + 1),
            digest(command_offset as u8 + 1),
            defaults.revision,
            digest(writer),
            values,
            1_800_000_000_001 + command_offset as i64,
        )
        .unwrap();
}

fn command(value: u64) -> CommandId {
    CommandId::from_parts(45, value)
}
fn digest(value: u8) -> ContentDigest {
    ContentDigest::from_bytes([value; 32])
}
