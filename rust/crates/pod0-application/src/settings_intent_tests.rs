use pod0_domain::ProductSettingsValues;

use crate::{
    MAX_PRODUCT_SETTING_INTENTS, PlaybackSettingToggle, ProductModelSlot, ProductSettingIntent,
    ProductSettingIntentError, apply_product_setting_intents,
};

#[test]
fn typed_intents_change_only_their_named_settings() {
    let current = ProductSettingsValues::default();
    let changed = apply_product_setting_intents(
        &current,
        &[
            ProductSettingIntent::SelectModel {
                slot: ProductModelSlot::Categorization,
                model_id: "provider/category".into(),
                model_name: "Category".into(),
            },
            ProductSettingIntent::SetPlaybackToggle {
                setting: PlaybackSettingToggle::AutoSkipAds,
                enabled: true,
            },
        ],
    )
    .unwrap();
    assert_eq!(changed.categorization_model, "provider/category");
    assert_eq!(changed.categorization_model_name, "Category");
    assert!(changed.auto_skip_ads);
    assert_eq!(changed.agent_initial_model, current.agent_initial_model);
    assert_eq!(changed.skip_forward_seconds, current.skip_forward_seconds);
}

#[test]
fn empty_oversized_and_duplicate_batches_are_rejected() {
    let current = ProductSettingsValues::default();
    assert_eq!(
        apply_product_setting_intents(&current, &[]),
        Err(ProductSettingIntentError::Empty)
    );
    let repeated = ProductSettingIntent::SetPlaybackToggle {
        setting: PlaybackSettingToggle::AutoPlayNext,
        enabled: false,
    };
    assert_eq!(
        apply_product_setting_intents(&current, &[repeated.clone(), repeated.clone()]),
        Err(ProductSettingIntentError::DuplicateTarget)
    );
    assert_eq!(
        apply_product_setting_intents(&current, &vec![repeated; MAX_PRODUCT_SETTING_INTENTS + 1]),
        Err(ProductSettingIntentError::TooMany)
    );
}
