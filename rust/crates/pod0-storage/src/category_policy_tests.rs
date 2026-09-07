use pod0_domain::{
    AutoDownloadMode, AutoDownloadPolicy, CategoryAutoDownloadSource, CategoryOrigin,
    CategoryRevision, CategorySettings, PodcastId,
};

use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, LibraryStore, MigrationClock, StorageError,
};

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        NOW + 100
    }
}
use crate::category_store_test_support::{NOW, as_podcast, command, fp, item, store};

fn policy(mode: AutoDownloadMode, wifi_only: bool) -> AutoDownloadPolicy {
    AutoDownloadPolicy { mode, wifi_only }
}

#[test]
fn membership_projects_rust_owned_category_defaults() {
    let (_fixture, store) = store();
    let (_, category_id) = store
        .create_category(
            command(1),
            &fp("membership-category"),
            "Research",
            "Long-form research shows.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    store
        .tag_category_items(
            command(2),
            &fp("membership-tag"),
            category_id,
            &[item(0xA1)],
            &[],
            as_podcast,
            NOW + 1,
        )
        .unwrap();

    let category = store.category_snapshot().unwrap().categories.remove(0);
    assert_eq!(category.podcast_ids(), vec![item(0xA1)]);
    assert_eq!(category.settings, CategorySettings::default());
}

#[test]
fn category_override_wins_and_absent_override_uses_subscription_default() {
    let (_fixture, store) = store();
    let (_, category_id) = store
        .create_category(
            command(3),
            &fp("policy-category"),
            "Daily",
            "Shows checked every day.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    let podcast_id = PodcastId::from_bytes([0xA2; 16]);
    store
        .tag_category_items(
            command(4),
            &fp("policy-tag"),
            category_id,
            &[item(0xA2)],
            &[],
            as_podcast,
            NOW + 1,
        )
        .unwrap();
    let subscription = policy(AutoDownloadMode::AllNew, true);
    let inherited = store
        .effective_category_auto_download(podcast_id, subscription)
        .unwrap();
    assert_eq!(inherited.policy, subscription);
    assert_eq!(inherited.source, CategoryAutoDownloadSource::Subscription);

    let current = store.category_snapshot().unwrap().categories[0].revision;
    let override_policy = policy(AutoDownloadMode::Latest { count: 4 }, false);
    store
        .update_category_settings(
            command(5),
            &fp("policy-override"),
            category_id,
            current,
            CategorySettings {
                auto_download_override: Some(override_policy),
                rag_enabled: false,
                notifications_enabled: true,
            },
            NOW + 2,
        )
        .unwrap();
    let resolved = store
        .effective_category_auto_download(podcast_id, subscription)
        .unwrap();
    assert_eq!(resolved.policy, override_policy);
    assert_eq!(
        resolved.source,
        CategoryAutoDownloadSource::Category { category_id }
    );
    assert!(resolved.conflicting_category_ids.is_empty());
}

#[test]
fn conflicting_overrides_are_deterministic_and_stale_writes_fail_closed() {
    let (_fixture, store) = store();
    let (_, older) = store
        .create_category(
            command(6),
            &fp("older-category"),
            "Older",
            "Older membership.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    let (_, newer) = store
        .create_category(
            command(7),
            &fp("newer-category"),
            "Newer",
            "Newer membership.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    for (seed, category_id, observed_at) in [(8, older, NOW + 1), (9, newer, NOW + 2)] {
        store
            .tag_category_items(
                command(seed),
                &fp(&format!("tag-{seed}")),
                category_id,
                &[item(0xA3)],
                &[],
                as_podcast,
                observed_at,
            )
            .unwrap();
    }
    let off = policy(AutoDownloadMode::Off, false);
    let latest = policy(AutoDownloadMode::Latest { count: 2 }, true);
    for (seed, category_id, override_policy) in [(10, older, off), (11, newer, latest)] {
        store
            .update_category_settings(
                command(seed),
                &fp(&format!("settings-{seed}")),
                category_id,
                CategoryRevision::new(2),
                CategorySettings {
                    auto_download_override: Some(override_policy),
                    ..CategorySettings::default()
                },
                NOW + i64::from(seed),
            )
            .unwrap();
    }

    let stale = store.update_category_settings(
        command(12),
        &fp("stale-settings"),
        newer,
        CategoryRevision::new(2),
        CategorySettings::default(),
        NOW + 12,
    );
    assert!(matches!(stale, Err(StorageError::RevisionConflict)));

    let resolved = store
        .effective_category_auto_download(
            PodcastId::from_bytes([0xA3; 16]),
            policy(AutoDownloadMode::AllNew, true),
        )
        .unwrap();
    assert_eq!(resolved.policy, latest);
    assert_eq!(
        resolved.source,
        CategoryAutoDownloadSource::Category { category_id: newer }
    );
    assert_eq!(resolved.conflicting_category_ids, vec![older]);
}

#[test]
fn schema_47_backfills_existing_categories_with_exact_defaults() {
    let (fixture, store) = store();
    store
        .create_category(
            command(13),
            &fp("pre-policy-category"),
            "Existing",
            "Created before category settings storage.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    drop(store);
    rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .execute_batch(
            "DROP TABLE pod0_category_settings;
             UPDATE pod0_schema_versions SET version=46 WHERE component='kernel';
             PRAGMA user_version=46;",
        )
        .unwrap();

    CoreStoreMigrator::new(FixedClock)
        .migrate(
            &fixture.target,
            CURRENT_SCHEMA_VERSION,
            &fixture.target.with_extension("category-v46-backup.sqlite"),
            command(14),
        )
        .unwrap();
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(
        reopened.category_snapshot().unwrap().categories[0].settings,
        CategorySettings::default()
    );
}
