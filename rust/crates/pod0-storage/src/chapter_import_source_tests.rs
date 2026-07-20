use rusqlite::params;

use crate::chapter_import_test_support::{
    ChapterImportFixture, EPISODE_ID, PODCAST_ID, SECOND_EPISODE_ID,
};

#[test]
fn episode_adjunct_distinguishes_explicit_empty_ads_from_not_evaluated() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &episode_with_chapters(EPISODE_ID, true, false),
    );
    fixture.insert_episode(
        SECOND_EPISODE_ID,
        PODCAST_ID,
        &episode_with_chapters(SECOND_EPISODE_ID, false, false),
    );

    assert_eq!(fixture.inspect().canonical_artifact_count, 2);
    fixture.stage(1_800_000_000_000);
    let verified = fixture.verify(1_800_000_000_001);
    assert_eq!(verified.verified_artifact_count, 2);
    fixture.import(1_800_000_000_002);

    let connection = fixture.target_connection();
    let mut statement = connection
        .prepare(
            "SELECT ad_span_evaluation_code,ad_span_count FROM pod0_chapter_artifacts \
             ORDER BY episode_id",
        )
        .unwrap();
    let values = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(values, [(2, 0), (1, 0)]);
}

#[test]
fn publisher_generated_enriched_and_agent_sources_keep_distinct_provenance() {
    let fixture = ChapterImportFixture::new_v1();
    let sources = [
        (EPISODE_ID, "publisher:feed-v1", false),
        (SECOND_EPISODE_ID, "generated", true),
        (
            "44444444-4444-4444-4444-444444444444",
            "publisherEnriched:feed-v2",
            true,
        ),
    ];
    for (index, (episode, origin, generated)) in sources.iter().enumerate() {
        fixture.insert_episode(episode, PODCAST_ID, &episode_without_adjunct(episode));
        fixture.insert_workflow_artifact(
            "chapters",
            episode,
            &format!("input-{index}"),
            &format!("output-{index}"),
            origin,
            "available",
            1_800_000_000.0 + index as f64,
            true,
            &workflow_chapters(&format!("Chapter {index}"), *generated),
        );
    }
    let agent_episode = "55555555-5555-5555-5555-555555555555";
    fixture.insert_episode(
        agent_episode,
        PODCAST_ID,
        &episode_with_chapters(agent_episode, true, true),
    );

    let plan = fixture.inspect();
    assert_eq!(fixture.inspect(), plan);
    assert_eq!(plan.canonical_artifact_count, 4);
    assert_eq!(plan.selected_count, 4);
    fixture.stage(1_800_000_010_000);
    fixture.verify(1_800_000_010_001);
    fixture.import(1_800_000_010_002);

    let connection = fixture.target_connection();
    let mut statement = connection
        .prepare(
            "SELECT source_code,legacy_source_code FROM pod0_chapter_artifacts \
             ORDER BY source_code",
        )
        .unwrap();
    let values = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(values, [(1, 3), (2, 3), (3, 3), (4, 1)]);
}

#[test]
fn v0_winner_policy_imports_complete_history_and_one_selection() {
    let fixture = ChapterImportFixture::new_v0();
    fixture.insert_episode(EPISODE_ID, PODCAST_ID, &episode_without_adjunct(EPISODE_ID));
    fixture.insert_workflow_artifact(
        "chapters",
        EPISODE_ID,
        "old-input",
        "old-output",
        "publisher:old",
        "stale",
        1_700_000_000.0,
        false,
        &workflow_chapters("Old", false),
    );
    fixture.insert_workflow_artifact(
        "chapters",
        EPISODE_ID,
        "current-input",
        "current-output",
        "generated",
        "available",
        1_800_000_000.0,
        false,
        &workflow_chapters("Current", true),
    );
    fixture.insert_workflow_artifact(
        "adSegments",
        EPISODE_ID,
        "current-input",
        "ads-output",
        "generated",
        "available",
        1_800_000_000.0,
        false,
        r#"[{"start":10.0,"end":20.0,"kind":"midroll"}]"#,
    );

    let plan = fixture.inspect();
    assert_eq!(plan.canonical_artifact_count, 2);
    assert_eq!(plan.evidence_count, 3);
    assert_eq!(plan.selected_count, 1);
    fixture.stage(1_800_000_020_000);
    let verified = fixture.verify(1_800_000_020_001);
    assert_eq!(verified.verified_artifact_count, 2);
    assert_eq!(verified.verified_ad_span_count, 1);
    fixture.import(1_800_000_020_002);

    let connection = fixture.target_connection();
    let counts: (i64, i64) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM pod0_chapter_artifacts),\
             (SELECT COUNT(*) FROM pod0_chapter_selections)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 1));
    let selected_revision: String = connection
        .query_row(
            "SELECT a.source_revision FROM pod0_chapter_selections s \
             JOIN pod0_chapter_artifacts a ON a.artifact_id=s.artifact_id",
            params![],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(selected_revision, "current-input");
}

pub(crate) fn episode_without_adjunct(episode: &str) -> String {
    format!(r#"{{"id":"{episode}","podcastID":"{PODCAST_ID}"}}"#)
}

pub(crate) fn episode_with_chapters(episode: &str, explicit_ads: bool, agent: bool) -> String {
    let ads = if explicit_ads {
        r#", "adSegments":[]"#
    } else {
        ""
    };
    let generation = if agent {
        r#", "generationSource":{"type":"inAppChat","conversationID":"66666666-6666-6666-6666-666666666666"}"#
    } else {
        ""
    };
    format!(
        r#"{{"id":"{episode}","podcastID":"{PODCAST_ID}","pubDate":"2026-07-20T00:00:00Z","duration":120.0,"chapters":[{{"startTime":0.0,"endTime":60.0,"title":"Opening","includeInTableOfContents":true,"isAIGenerated":{agent}}}] {ads}{generation}}}"#
    )
}

pub(crate) fn workflow_chapters(title: &str, generated: bool) -> String {
    format!(
        r#"[{{"startTime":0.0,"endTime":60.0,"title":"{title}","includeInTableOfContents":true,"isAIGenerated":{generated}}}]"#
    )
}
