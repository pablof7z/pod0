use rusqlite::Connection;

use crate::migration_tests::Fixture;

#[test]
fn subscription_transcript_policy_migration_preserves_existing_rows_as_automatic() {
    let fixture = Fixture::new();
    fixture.migrate_to(28, 1).unwrap();
    Connection::open(&fixture.store)
        .unwrap()
        .execute_batch(
            "PRAGMA foreign_keys=ON;
             INSERT INTO pod0_listening_imports(
                 import_id,source_kind,source_hash,source_generation,podcast_count,
                 subscription_count,episode_count,backup_byte_count,target_revision,state,
                 verified_at_ms
             ) VALUES(
                 x'01010101010101010101010101010101',1,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 1,1,1,0,1,1,'verified',1
             );
             INSERT INTO pod0_podcasts(
                 podcast_id,kind_code,kind_wire_code,feed_url,feed_key_v1,title,author,
                 image_url,description,language,categories_json,discovered_at_ms,
                 title_is_placeholder,last_refreshed_at_ms,etag,last_modified,
                 source_import_id,library_visible
             ) VALUES(
                 x'02020202020202020202020202020202',1,NULL,NULL,NULL,'Fixture','',
                 NULL,'',NULL,'[]',1,0,NULL,NULL,NULL,
                 x'01010101010101010101010101010101',1
             );
             INSERT INTO pod0_subscriptions(
                 podcast_id,subscribed_at_ms,auto_download_code,auto_download_wire_code,
                 auto_download_latest_count,wifi_only,notifications_enabled,
                 default_playback_rate_permille,source_import_id
             ) VALUES(
                 x'02020202020202020202020202020202',1,3,NULL,NULL,1,1,NULL,
                 x'01010101010101010101010101010101'
             );",
        )
        .unwrap();

    let report = fixture.migrate_to(32, 2).unwrap();
    assert_eq!(report.applied_versions, [29, 30, 31, 32]);
    let connection = Connection::open(&fixture.store).unwrap();
    let stored: (i64, Option<i64>) = connection
        .query_row(
            "SELECT transcript_start_policy_code,transcript_start_policy_wire_code
             FROM pod0_subscriptions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, (1, None));
}
