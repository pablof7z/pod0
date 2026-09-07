use pod0_domain::{CommandId, ContentDigest, ProductSettingsValues};

use crate::Pod0Facade;

#[test]
fn facade_imports_once_and_routes_later_mutations_through_rust() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("pod0.sqlite")
        .to_string_lossy()
        .into_owned();
    let facade = Pod0Facade::create(path).unwrap();
    let mut imported = ProductSettingsValues::default();
    imported.agent_display_name = "Imported".into();

    let authority = facade
        .import_legacy_product_settings(command(1), 12, digest(1), imported)
        .unwrap();
    assert!(authority.authoritative);
    let current = authority.settings.unwrap();
    assert_eq!(current.values.agent_display_name, "Imported");

    let mut changed = current.values.clone();
    changed.agent_display_name = "Committed".into();
    let projection = facade
        .set_product_settings(command(2), current.revision, digest(2), changed)
        .unwrap();
    assert_eq!(
        projection.settings.unwrap().values.agent_display_name,
        "Committed"
    );
}

fn command(value: u64) -> CommandId {
    CommandId::from_parts(59, value)
}

fn digest(value: u8) -> ContentDigest {
    ContentDigest::from_bytes([value; 32])
}
