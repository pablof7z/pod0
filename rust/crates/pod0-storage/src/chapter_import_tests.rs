use pod0_domain::CommandId;
use rusqlite::Connection;

use crate::chapter_import_test_support::{ChapterImportFixture, FixedClock};
use crate::{ChapterImportState, ChapterImporter, inspect_legacy_chapter_source};

#[test]
fn episode_adjunct_stages_verifies_imports_and_activates_authority_atomically() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        "11111111-1111-1111-1111-111111111111",
        "22222222-2222-2222-2222-222222222222",
        r#"{
          "id":"11111111-1111-1111-1111-111111111111",
          "podcastID":"22222222-2222-2222-2222-222222222222",
          "pubDate":"2026-07-19T00:00:00Z",
          "duration":120.0,
          "chapters":[
            {"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","startTime":0.0,
             "endTime":60.0,"title":"Opening","includeInTableOfContents":true,
             "isAIGenerated":false},
            {"id":"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb","startTime":60.0,
             "title":"Deep dive","includeInTableOfContents":true,
             "isAIGenerated":false,"summary":"The useful part.",
             "imageURL":"https://example.com/chapter.jpg",
             "linkURL":"https://example.com/source",
             "sourceEpisodeID":"cccccccc-cccc-cccc-cccc-cccccccccccc"}
          ],
          "adSegments":[]
        }"#,
    );
    let plan = inspect_legacy_chapter_source(&fixture.source, &fixture.artifacts).unwrap();
    assert_eq!(plan.evidence_count, 1);
    assert_eq!(plan.canonical_artifact_count, 1);
    assert_eq!(plan.selected_count, 1);
    assert_eq!(plan.blocked_count, 0);

    let importer = ChapterImporter::new(FixedClock(1_800_000_000_000));
    let staged = importer
        .stage(
            &fixture.source,
            &fixture.artifacts,
            &fixture.legacy_backup,
            &fixture.target,
            &fixture.schema_backup,
            &plan,
            CommandId::from_parts(9, 9),
            CommandId::from_parts(8, 8),
        )
        .unwrap();
    assert_eq!(staged.state, ChapterImportState::Staged);
    assert!(
        importer
            .stage(
                &fixture.source,
                &fixture.artifacts,
                &fixture.legacy_backup,
                &fixture.target,
                &fixture.schema_backup,
                &plan,
                CommandId::from_parts(9, 9),
                CommandId::from_parts(8, 8),
            )
            .unwrap()
            .reused_existing
    );
    let verified = importer
        .verify(
            &fixture.source,
            &fixture.artifacts,
            &fixture.legacy_backup,
            &fixture.target,
            CommandId::from_parts(9, 9),
        )
        .unwrap();
    assert_eq!(verified.report.state, ChapterImportState::Verified);
    assert_eq!(verified.verified_chapter_count, 2);
    assert_eq!(verified.verified_ad_span_count, 0);
    let imported = importer
        .commit(
            &fixture.source,
            &fixture.artifacts,
            &fixture.target,
            CommandId::from_parts(9, 9),
        )
        .unwrap();
    assert_eq!(imported.state, ChapterImportState::Imported);
    let connection = Connection::open(&fixture.target).unwrap();
    let authority: (bool, Option<Vec<u8>>) = connection
        .query_row(
            "SELECT authority_active,authority_import_id FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        authority,
        (
            true,
            Some(CommandId::from_parts(9, 9).into_bytes().to_vec())
        )
    );
    let selection_count: u32 = connection
        .query_row("SELECT COUNT(*) FROM pod0_chapter_selections", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(selection_count, 1);
    let selected = crate::chapter_store_read_selection::read_selected_chapter_artifact(
        &connection,
        pod0_domain::EpisodeId::from_bytes([0x11; 16]),
    )
    .unwrap()
    .unwrap();
    let linked = &selected.artifact.chapters[1];
    assert_eq!(linked.summary.as_deref(), Some("The useful part."));
    assert_eq!(
        linked.image_url.as_deref(),
        Some("https://example.com/chapter.jpg")
    );
    assert_eq!(
        linked.link_url.as_deref(),
        Some("https://example.com/source")
    );
    assert_eq!(
        linked.source_episode_id,
        Some(pod0_domain::EpisodeId::from_bytes([0xcc; 16]))
    );
}
