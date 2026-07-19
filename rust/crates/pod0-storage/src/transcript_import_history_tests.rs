use rusqlite::Connection;

use crate::legacy_transcript_db::orphan_transcript_podcast_id;
use crate::listening_import_test_support::EPISODE_ID;
use crate::transcript_import_test_support::{TranscriptImportFixture, command};
use crate::{TranscriptStore, read_transcript_import_entries};

#[test]
fn complete_history_and_orphan_selection_survive_cutover_and_reopen() {
    const ORPHAN_EPISODE: &str = "77777777-7777-7777-7777-777777777777";
    let fixture = TranscriptImportFixture::current();
    fixture.add_current_artifact(
        EPISODE_ID,
        "history.json",
        "An older historical transcript",
        "input-history-v0",
        "output-history-v0",
        false,
    );
    fixture.add_current_artifact(
        ORPHAN_EPISODE,
        "orphan.json",
        "A recovered orphan transcript",
        "input-orphan-v1",
        "output-orphan-v1",
        true,
    );

    let plan = fixture.plan();
    assert_eq!(plan.artifact_count, 3);
    assert_eq!(plan.selected_count, 2);
    let staged = fixture.stage(command(22)).unwrap();
    assert_eq!(staged.backup.artifact_count, 3);
    let verified = fixture.verify(command(22)).unwrap();
    assert_eq!(verified.verified_artifact_count, 3);
    fixture.commit(command(22)).unwrap();

    let entries = read_transcript_import_entries(&fixture.import.target, command(22), 0, 10)
        .unwrap()
        .items;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries.iter().filter(|entry| entry.is_selected).count(), 2);
    let history = entries
        .iter()
        .find(|entry| {
            entry.episode_id == pod0_domain::EpisodeId::from_bytes([0x22; 16]) && !entry.is_selected
        })
        .unwrap();
    let connection = Connection::open(&fixture.import.target).unwrap();
    let history_artifact = crate::transcript_store_read_artifact::read_artifact_by_id(
        &connection,
        history.artifact_id,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        history_artifact.segments[0].text,
        "An older historical transcript"
    );
    assert_eq!(count(&connection, "pod0_transcript_artifacts"), 3);
    assert_eq!(count(&connection, "pod0_transcript_selection"), 2);
    let orphan_visible: bool = connection
        .query_row(
            "SELECT library_visible FROM pod0_podcasts WHERE podcast_id=?1",
            [orphan_transcript_podcast_id().into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!orphan_visible);

    drop(connection);
    let reopened = TranscriptStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        selected_text(&reopened, [0x22; 16]),
        "Small habits become durable"
    );
    assert_eq!(
        selected_text(&reopened, [0x77; 16]),
        "A recovered orphan transcript"
    );
}

#[test]
fn large_history_import_is_complete_and_its_entry_projection_stays_bounded() {
    const HISTORY_COUNT: u32 = 1_000;
    let fixture = TranscriptImportFixture::current();
    for index in 0..HISTORY_COUNT {
        fixture.add_current_artifact(
            EPISODE_ID,
            &format!("history-{index}.json"),
            &format!("Historical transcript {index}"),
            &format!("input-history-{index}"),
            &format!("output-history-{index}"),
            false,
        );
    }

    let plan = fixture.plan();
    assert_eq!(plan.artifact_count, HISTORY_COUNT + 1);
    assert_eq!(plan.selected_count, 1);
    let staged = fixture.stage(command(23)).unwrap();
    assert_eq!(staged.backup.artifact_count, HISTORY_COUNT + 1);
    assert_eq!(
        fixture.verify(command(23)).unwrap().verified_artifact_count,
        HISTORY_COUNT + 1
    );

    let mut offset = 0_u32;
    let mut observed = 0_usize;
    loop {
        let page = read_transcript_import_entries(&fixture.import.target, command(23), offset, 200)
            .unwrap();
        assert!(page.items.len() <= 200);
        observed += page.items.len();
        if !page.has_more {
            break;
        }
        offset += u32::try_from(page.items.len()).unwrap();
    }
    assert_eq!(observed, (HISTORY_COUNT + 1) as usize);

    fixture.commit(command(23)).unwrap();
    let connection = Connection::open(&fixture.import.target).unwrap();
    assert_eq!(
        count(&connection, "pod0_transcript_artifacts"),
        HISTORY_COUNT + 1
    );
    assert_eq!(count(&connection, "pod0_transcript_selection"), 1);
    drop(connection);
    let reopened = TranscriptStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        selected_text(&reopened, [0x22; 16]),
        "Small habits become durable"
    );
}

fn selected_text(store: &TranscriptStore, episode: [u8; 16]) -> String {
    store
        .selected_artifact(pod0_domain::EpisodeId::from_bytes(episode))
        .unwrap()
        .unwrap()
        .segments[0]
        .text
        .clone()
}

fn count(connection: &Connection, table: &str) -> u32 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
