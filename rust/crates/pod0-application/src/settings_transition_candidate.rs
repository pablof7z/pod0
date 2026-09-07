use pod0_domain::{
    PRODUCT_SETTINGS_SCHEMA_VERSION, ProductSettings, ProductSettingsValues, SettingsWriterVersion,
};

use crate::{SettingsChange, TransitionPlanError};

pub(super) fn settings_candidate(
    current: Option<&ProductSettings>,
    change: SettingsChange,
) -> Result<(u32, SettingsWriterVersion, ProductSettingsValues), TransitionPlanError> {
    Ok(match change {
        SettingsChange::Defaults { writer_id } => (
            PRODUCT_SETTINGS_SCHEMA_VERSION,
            SettingsWriterVersion {
                counter: 0,
                writer_id,
            },
            ProductSettingsValues::default(),
        ),
        SettingsChange::Local {
            writer_id, values, ..
        } => (
            PRODUCT_SETTINGS_SCHEMA_VERSION,
            SettingsWriterVersion {
                counter: current
                    .map(|value| value.writer_version.counter)
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(TransitionPlanError::RevisionExhausted)?,
                writer_id,
            },
            values,
        ),
        SettingsChange::Remote {
            schema_version,
            writer_version,
            values,
        } => (schema_version, writer_version, values),
    })
}
