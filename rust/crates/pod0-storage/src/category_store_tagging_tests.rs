use pod0_domain::{CategoryItemKind, CategoryOrigin, EpisodeId, LibraryItemId};

use crate::StorageError;
use crate::category_store_test_support::{NOW, as_podcast, command, fp, item, store};

#[test]
fn tagging_is_additive_deduplicated_and_reversible() {
    let (_fixture, store) = store();
    let (_, id) = store
        .create_category(
            command(6),
            &fp("fp-6"),
            "Marketing",
            "Positioning and growth.",
            None,
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();

    let (_, added, removed) = store
        .tag_category_items(
            command(7),
            &fp("fp-7"),
            id,
            &[item(0xA1), item(0xA2), item(0xA1)],
            &[],
            as_podcast,
            NOW + 1,
        )
        .unwrap();
    // The duplicate in the same call is absorbed rather than double-counted.
    assert_eq!((added, removed), (2, 0));
    assert_eq!(
        store.category_snapshot().unwrap().categories[0].members.len(),
        2
    );

    let (_, added, removed) = store
        .tag_category_items(
            command(8),
            &fp("fp-8"),
            id,
            &[],
            &[item(0xA1)],
            as_podcast,
            NOW + 2,
        )
        .unwrap();
    assert_eq!((added, removed), (0, 1));

    let category = store.category_snapshot().unwrap().categories.remove(0);
    assert_eq!(category.podcast_ids(), vec![item(0xA2)]);
    assert!(category.episode_ids().is_empty());
}

#[test]
fn an_item_belongs_to_every_category_that_claims_it() {
    let (_fixture, store) = store();
    let (_, marketing) = store
        .create_category(
            command(9),
            &fp("fp-9"),
            "Marketing",
            "Positioning and growth.",
            None,
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();
    let (_, philosophy) = store
        .create_category(
            command(10),
            &fp("fp-10"),
            "Philosophy",
            "Meaning and mind.",
            None,
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();

    for (seed, id) in [(11_u8, marketing), (12, philosophy)] {
        store
            .tag_category_items(
                command(seed),
                &fp(&format!("fp-tag-{seed}")),
                id,
                &[item(0xB1)],
                &[],
                as_podcast,
                NOW + 1,
            )
            .unwrap();
    }

    let mut owners = store.categories_for_item(item(0xB1)).unwrap();
    owners.sort_by_key(|id| id.into_bytes());
    let mut expected = vec![marketing, philosophy];
    expected.sort_by_key(|id| id.into_bytes());
    assert_eq!(owners, expected);
}

#[test]
fn tagging_rejects_an_id_it_cannot_resolve_instead_of_dropping_it() {
    let (_fixture, store) = store();
    let (_, id) = store
        .create_category(
            command(13),
            &fp("fp-13"),
            "Marketing",
            "Positioning and growth.",
            None,
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();

    assert!(matches!(
        store.tag_category_items(
            command(14),
            &fp("fp-14"),
            id,
            &[item(0xC1)],
            &[],
            |_| None,
            NOW + 1,
        ),
        Err(StorageError::EntityNotFound)
    ));
    // The whole command rolled back: nothing was half-applied.
    assert!(
        store.category_snapshot().unwrap().categories[0]
            .members
            .is_empty()
    );
}

#[test]
fn deleting_a_category_drops_its_membership_but_not_its_items() {
    let (_fixture, store) = store();
    let (_, id) = store
        .create_category(
            command(15),
            &fp("fp-15"),
            "Marketing",
            "Positioning and growth.",
            None,
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();
    store
        .tag_category_items(
            command(16),
            &fp("fp-16"),
            id,
            &[item(0xD1)],
            &[],
            as_podcast,
            NOW + 1,
        )
        .unwrap();

    store
        .delete_category(command(17), &fp("fp-17"), id, NOW + 2)
        .unwrap();

    assert!(store.category_snapshot().unwrap().categories.is_empty());
    assert!(store.categories_for_item(item(0xD1)).unwrap().is_empty());
    // A second delete of the same id is not found rather than silently ok,
    // so a caller cannot mistake a stale id for a successful removal.
    assert!(matches!(
        store.delete_category(command(18), &fp("fp-18"), id, NOW + 3),
        Err(StorageError::EntityNotFound)
    ));
}

#[test]
fn tagging_an_episode_projects_the_durable_transition_to_episode_diagnostics() {
    let (fixture, store) = store();
    let (_, id) = store
        .create_category(
            command(22),
            &fp("fp-22"),
            "Episode research",
            "Curated episodes.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    let episode_id = EpisodeId::from_bytes([0x22; 16]);
    let item_id = LibraryItemId::from_bytes(episode_id.into_bytes());
    store
        .tag_category_items(
            command(23),
            &fp("fp-23"),
            id,
            &[item_id],
            &[],
            |_| Some(CategoryItemKind::Episode),
            NOW + 1,
        )
        .unwrap();

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_activity_facts WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}
