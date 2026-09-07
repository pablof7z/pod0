use pod0_application::{PlaybackSettingToggle, ProductSettingIntent};
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
    let imported = ProductSettingsValues {
        agent_display_name: "Imported".into(),
        ..ProductSettingsValues::default()
    };

    let authority = facade
        .import_legacy_product_settings(command(1), 12, digest(1), imported)
        .unwrap();
    assert!(authority.authoritative);
    let current = authority.settings.unwrap();
    assert_eq!(current.values.agent_display_name, "Imported");

    let projection = facade
        .apply_product_setting_intents(
            command(2),
            current.revision,
            digest(2),
            vec![ProductSettingIntent::SetPlaybackToggle {
                setting: PlaybackSettingToggle::AutoSkipAds,
                enabled: true,
            }],
        )
        .unwrap();
    let committed = projection.settings.unwrap();
    assert!(committed.values.auto_skip_ads);
    assert_eq!(committed.values.agent_display_name, "Imported");
}

fn command(value: u64) -> CommandId {
    CommandId::from_parts(59, value)
}

fn digest(value: u8) -> ContentDigest {
    ContentDigest::from_bytes([value; 32])
}
