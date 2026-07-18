use std::fs;

use pod0_domain::{CompletionCause, CompletionStatus, DownloadArtifactStatus};
use rusqlite::Connection;

use crate::listening_import_test_support::*;
use crate::{
    LegacySourceKind, ListeningImporter, StorageError, inspect_legacy_listening_source,
    read_listening_import,
};

#[test]
fn current_sqlite_is_staged_losslessly_and_retry_is_idempotent() {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    let source_bytes = fs::read(&fixture.source).unwrap();
    let plan = fixture.plan();
    assert_eq!(plan.source_kind, LegacySourceKind::SwiftSqlite);

    let report = fixture.stage(&plan).unwrap();
    assert!(report.staged && !report.reused_existing);
    assert_eq!(
        (
            report.plan.podcast_count,
            report.plan.subscription_count,
            report.plan.episode_count
        ),
        (1, 1, 1)
    );
    let verification = read_listening_import(&fixture.target, id(1)).unwrap();
    let podcast = &verification.snapshot.podcasts[0];
    assert_eq!(
        podcast.feed_identity.as_ref().unwrap().comparison_key,
        "https://example.test/feed"
    );
    assert_eq!(podcast.categories, ["Tech", "News"]);
    let imported_episode = &verification.snapshot.episodes[0];
    assert!(imported_episode.is_starred);
    assert_eq!(
        imported_episode.listening.resume_position_milliseconds,
        32_250
    );
    assert_eq!(
        imported_episode.listening.completion,
        CompletionStatus::Completed {
            cause: CompletionCause::LegacyPlayedFlag
        }
    );
    assert!(matches!(
        imported_episode.download,
        DownloadArtifactStatus::Available {
            byte_count: 4096,
            ..
        }
    ));
    assert_eq!(
        verification.snapshot.playback.active_episode_id,
        Some(imported_episode.episode_id)
    );
    assert_eq!(verification.snapshot.playback.rate.value, 1_250);
    assert!(!verification.snapshot.playback.auto_play_next);
    assert_eq!(fs::read(&fixture.source).unwrap(), source_bytes);
    assert_eq!(
        inspect_legacy_listening_source(&fixture.source_backup).unwrap(),
        plan
    );
    assert_eq!(
        Connection::open(&fixture.source)
            .unwrap()
            .query_row("SELECT value FROM workflow_sentinel", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "preserve-me"
    );
    assert_eq!(
        Connection::open(&fixture.target)
            .unwrap()
            .query_row(
                "SELECT state FROM pod0_domain_cutovers WHERE domain='listening'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "staged"
    );

    let retry = fixture.stage(&plan).unwrap();
    assert!(retry.reused_existing);
    assert_eq!(retry.plan, plan);
}

#[test]
fn legacy_json_and_empty_install_have_typed_staged_projections() {
    let legacy = ImportFixture::new();
    create_legacy_json(&legacy.source);
    let legacy_plan = legacy.plan();
    assert_eq!(legacy_plan.source_kind, LegacySourceKind::LegacyJson);
    legacy.stage(&legacy_plan).unwrap();
    let imported = read_listening_import(&legacy.target, id(1))
        .unwrap()
        .snapshot;
    assert_eq!(imported.podcasts[0].title, "Legacy");
    assert!(imported.episodes[0].is_starred);
    assert_eq!(imported.playback.revision.value, 1);

    let empty = ImportFixture::new();
    create_sqlite_source(
        &empty.source,
        &serde_json::json!({
            "persistenceGeneration": 0, "podcasts": [], "subscriptions": [], "episodes": [], "settings": {}
        }),
        &[],
    );
    let plan = empty.plan();
    empty.stage(&plan).unwrap();
    let snapshot = read_listening_import(&empty.target, id(1))
        .unwrap()
        .snapshot;
    assert!(snapshot.podcasts.is_empty() && snapshot.episodes.is_empty());
    assert_eq!(snapshot.playback.revision.value, 1);
}

#[test]
fn interruption_rolls_back_listening_rows_and_retry_recovers() {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    let plan = fixture.plan();
    let importer = ListeningImporter::new(FixedClock);
    let error = importer
        .stage_with_observer(
            &fixture.source,
            &fixture.source_backup,
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(1),
            id(2),
            || Err(StorageError::Interrupted),
        )
        .unwrap_err();
    assert_eq!(error, StorageError::Interrupted);
    let connection = Connection::open(&fixture.target).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pod0_listening_imports", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM pod0_domain_cutovers WHERE domain='listening'",
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
        0
    );
    drop(connection);
    assert!(fixture.stage(&plan).unwrap().staged);
}

#[test]
fn large_library_import_remains_bounded_and_complete() {
    let fixture = ImportFixture::new();
    let episodes: Vec<_> = (0..2_000_u64)
        .map(|value| {
            let id_value = format!("00000000-0000-0000-{:04x}-{:012x}", value / 0x1_0000, value);
            episode(&id_value, &format!("guid-{value}"))
        })
        .collect();
    create_sqlite_source(&fixture.source, &current_metadata(99), &episodes);
    let plan = fixture.plan();
    assert_eq!(plan.episode_count, 2_000);
    fixture.stage(&plan).unwrap();
    assert_eq!(
        read_listening_import(&fixture.target, id(1))
            .unwrap()
            .snapshot
            .episodes
            .len(),
        2_000
    );
}
