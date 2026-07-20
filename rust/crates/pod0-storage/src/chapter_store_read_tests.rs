use crate::chapter_import_test_support::{ChapterImportFixture, EPISODE_ID, PODCAST_ID};
use crate::chapter_store_read_selection::read_selected_chapter_artifact;

#[test]
fn latest_imported_selection_rebuilds_and_verifies_the_exact_artifact() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        r#"{
          "id":"11111111-1111-1111-1111-111111111111",
          "podcastID":"22222222-2222-2222-2222-222222222222",
          "pubDate":"2026-07-19T00:00:00Z",
          "duration":120.0,
          "chapters":[
            {"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","startTime":0.0,
             "title":"Opening","includeInTableOfContents":true,"isAIGenerated":false},
            {"id":"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb","startTime":60.0,
             "title":"Deep dive","includeInTableOfContents":true,"isAIGenerated":false}
          ],
          "adSegments":[
            {"id":"cccccccc-cccc-cccc-cccc-cccccccccccc","start":10.0,"end":20.0,
             "kind":"midroll"}
          ]
        }"#,
    );
    fixture.stage(1_800_000_000_000);
    fixture.verify(1_800_000_000_001);
    fixture.import(1_800_000_000_002);

    let episode_id = pod0_domain::EpisodeId::from_bytes([0x11; 16]);
    let selected = read_selected_chapter_artifact(&fixture.target_connection(), episode_id)
        .unwrap()
        .unwrap();

    assert_eq!(selected.selection_revision.value, 1);
    assert_eq!(selected.artifact.episode_id, episode_id);
    assert_eq!(selected.artifact.chapters.len(), 2);
    assert_eq!(selected.artifact.ad_spans.len(), 1);
    assert_eq!(selected.artifact.verify_integrity(), Ok(()));
}
