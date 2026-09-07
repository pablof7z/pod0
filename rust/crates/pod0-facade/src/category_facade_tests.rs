use pod0_domain::{
    CategoryId, CategoryOrigin, CategoryReplacementInput, CategorySettings, CommandId,
    UnixTimestampMilliseconds,
};

use crate::Pod0Facade;

#[test]
fn facade_imports_categories_once_and_commits_typed_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("pod0.sqlite")
        .to_string_lossy()
        .into_owned();
    let facade = Pod0Facade::create(path).unwrap();
    let category_id = CategoryId::from_parts(7, 1);
    let input = CategoryReplacementInput {
        category_id,
        name: "Research".into(),
        description: "Episodes worth revisiting.".into(),
        color_hex: Some("#445566".into()),
        origin: CategoryOrigin::Generated,
        podcast_ids: Vec::new(),
        settings: CategorySettings::default(),
        generated_at: UnixTimestampMilliseconds::new(500),
    };

    let imported = facade
        .import_legacy_categories(command(1), 12, vec![input.clone()])
        .unwrap();
    assert!(imported.authoritative);
    assert_eq!(imported.categories.len(), 1);
    assert_eq!(imported.categories[0].slug, "research");

    let mut changed = imported.categories[0].settings;
    changed.rag_enabled = false;
    let updated = facade
        .set_category_settings(
            command(2),
            category_id,
            imported.categories[0].revision,
            changed,
        )
        .unwrap();
    assert!(!updated.categories[0].settings.rag_enabled);

    let ignored = facade
        .import_legacy_categories(command(3), 99, Vec::new())
        .unwrap();
    assert_eq!(ignored.categories[0].name, "Research");
}

#[test]
fn bounded_projection_reports_truncation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("pod0.sqlite")
        .to_string_lossy()
        .into_owned();
    let facade = Pod0Facade::create(path).unwrap();
    let imported = facade
        .import_legacy_categories(command(4), 0, Vec::new())
        .unwrap();
    assert!(imported.authoritative);
    assert!(!imported.truncated);
    assert_eq!(imported.total_podcast_members, 0);
}

fn command(value: u64) -> CommandId {
    CommandId::from_parts(61, value)
}
