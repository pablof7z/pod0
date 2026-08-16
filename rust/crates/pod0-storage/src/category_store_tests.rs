use pod0_domain::{CategoryId, CategoryOrigin, MAX_CATEGORIES};

use crate::category_store_test_support::{NOW, as_podcast, command, fp, item, store};
use crate::{CategoryEdit, StorageError};

#[test]
fn creating_a_category_derives_a_slug_and_is_idempotent_under_replay() {
    let (fixture, store) = store();
    let (revision, id) = store
        .create_category(
            command(1),
            &fp("fp-1"),
            "Technology Deep-Dives",
            "Long-form engineering conversations.",
            Some("#4A90D9"),
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();

    let snapshot = store.category_snapshot().unwrap();
    assert_eq!(snapshot.categories.len(), 1);
    assert_eq!(snapshot.categories[0].slug, "technology-deep-dives");
    assert_eq!(snapshot.categories[0].origin, CategoryOrigin::Agent);
    assert_eq!(snapshot.revision, revision);

    // Replaying the same command must land on the same row, not a second one.
    let (replayed, replayed_id) = store
        .create_category(
            command(1),
            &fp("fp-1"),
            "Technology Deep-Dives",
            "Long-form engineering conversations.",
            Some("#4A90D9"),
            CategoryOrigin::Agent,
            NOW + 1,
        )
        .unwrap();
    assert_eq!(replayed_id, id);
    assert_eq!(replayed, revision);
    assert_eq!(store.category_snapshot().unwrap().categories.len(), 1);
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    assert_eq!(activity_count(&connection), 5);
}

fn activity_count(connection: &rusqlite::Connection) -> i64 {
    connection
        .query_row("SELECT COUNT(*) FROM pod0_activity_facts", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn categories_reject_malformed_input_and_an_unbounded_taxonomy() {
    let (_fixture, store) = store();
    assert!(matches!(
        store.create_category(
            command(2),
            &fp("fp-2"),
            "  ",
            "desc",
            None,
            CategoryOrigin::Agent,
            NOW,
        ),
        Err(StorageError::InvalidCategory)
    ));
    assert!(matches!(
        store.create_category(
            command(3),
            &fp("fp-3"),
            "Philosophy",
            "desc",
            Some("4A90D9"),
            CategoryOrigin::Agent,
            NOW,
        ),
        Err(StorageError::InvalidCategory)
    ));

    for index in 0..MAX_CATEGORIES {
        let seed = u8::try_from(index + 10).unwrap();
        store
            .create_category(
                command(seed),
                &fp(&format!("fp-fill-{index}")),
                &format!("Category {index}"),
                "desc",
                None,
                CategoryOrigin::Generated,
                NOW,
            )
            .unwrap();
    }
    assert!(matches!(
        store.create_category(
            command(200),
            &fp("fp-over"),
            "One Too Many",
            "desc",
            None,
            CategoryOrigin::Agent,
            NOW,
        ),
        Err(StorageError::InvalidCategory)
    ));
}

#[test]
fn a_partial_edit_touches_only_the_fields_it_names() {
    let (_fixture, store) = store();
    let (_, id) = store
        .create_category(
            command(4),
            &fp("fp-4"),
            "Philosophy",
            "Meaning and mind.",
            Some("#101010"),
            CategoryOrigin::Agent,
            NOW,
        )
        .unwrap();

    store
        .update_category(
            command(5),
            &fp("fp-5"),
            id,
            &CategoryEdit {
                name: Some("Philosophy & Realization".to_owned()),
                ..CategoryEdit::default()
            },
            NOW + 1,
        )
        .unwrap();

    let category = store.category_snapshot().unwrap().categories.remove(0);
    assert_eq!(category.name, "Philosophy & Realization");
    assert_eq!(category.slug, "philosophy-realization");
    // Untouched fields survive the edit.
    assert_eq!(category.description, "Meaning and mind.");
    assert_eq!(category.color_hex.as_deref(), Some("#101010"));
    assert_eq!(category.revision.value, 2);
}

#[test]
fn a_name_with_no_ascii_falls_back_to_an_id_derived_slug() {
    let (_fixture, store) = store();
    let (_, id) = store
        .create_category(
            command(19),
            &fp("fp-19"),
            "哲学",
            "Meaning and mind.",
            None,
            CategoryOrigin::User,
            NOW,
        )
        .unwrap();
    let category = store.category_snapshot().unwrap().categories.remove(0);
    assert_eq!(category.slug, format!("c-{:016x}", id.high()));
    assert!(!category.slug.is_empty());
}

#[test]
fn editing_or_tagging_an_unknown_category_is_not_found() {
    let (_fixture, store) = store();
    let unknown = CategoryId::from_bytes([0xEE; 16]);
    assert!(matches!(
        store.update_category(
            command(20),
            &fp("fp-20"),
            unknown,
            &CategoryEdit {
                name: Some("Ghost".to_owned()),
                ..CategoryEdit::default()
            },
            NOW,
        ),
        Err(StorageError::EntityNotFound)
    ));
    assert!(matches!(
        store.tag_category_items(
            command(21),
            &fp("fp-21"),
            unknown,
            &[item(0xF1)],
            &[],
            as_podcast,
            NOW,
        ),
        Err(StorageError::EntityNotFound)
    ));
}
