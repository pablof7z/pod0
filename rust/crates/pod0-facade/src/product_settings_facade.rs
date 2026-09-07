use pod0_application::{ProductSettingIntent, apply_product_setting_intents};
use pod0_domain::{
    CommandId, ContentDigest, ProductSettings, ProductSettingsValues, SettingsWriterVersion,
    StateRevision,
};
use sha2::{Digest as _, Sha256};

use crate::{FacadeOpenError, Pod0Facade};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ProductSettingsAuthorityProjection {
    pub authoritative: bool,
    pub settings: Option<ProductSettings>,
}

#[uniffi::export]
impl Pod0Facade {
    pub fn product_settings_authority(
        &self,
    ) -> Result<ProductSettingsAuthorityProjection, FacadeOpenError> {
        let store = self.settings_store()?;
        settings_projection(&store)
    }

    pub fn import_legacy_product_settings(
        &self,
        command_id: CommandId,
        source_generation: u64,
        writer_id: ContentDigest,
        values: ProductSettingsValues,
    ) -> Result<ProductSettingsAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.settings_store_and_time()?;
        let fingerprint =
            settings_fingerprint(b"legacy-import", source_generation, writer_id, &values)?;
        store
            .import_legacy_product_settings(
                command_id,
                fingerprint,
                source_generation,
                writer_id,
                values,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        settings_projection(&store)
    }

    pub fn set_product_settings(
        &self,
        command_id: CommandId,
        expected_revision: StateRevision,
        writer_id: ContentDigest,
        values: ProductSettingsValues,
    ) -> Result<ProductSettingsAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.settings_store_and_time()?;
        let fingerprint =
            settings_fingerprint(b"local", expected_revision.value, writer_id, &values)?;
        store
            .set_local_product_settings(
                command_id,
                fingerprint,
                expected_revision,
                writer_id,
                values,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        settings_projection(&store)
    }

    pub fn apply_product_setting_intents(
        &self,
        command_id: CommandId,
        expected_revision: StateRevision,
        writer_id: ContentDigest,
        intents: Vec<ProductSettingIntent>,
    ) -> Result<ProductSettingsAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.settings_store_and_time()?;
        let current = store
            .product_settings()
            .map_err(FacadeOpenError::from)?
            .ok_or(FacadeOpenError::NotAuthoritative)?;
        let values = apply_product_setting_intents(&current.values, &intents)
            .map_err(|_| FacadeOpenError::StorageUnavailable)?;
        let fingerprint = settings_fingerprint(
            b"typed-intents",
            expected_revision.value,
            writer_id,
            &values,
        )?;
        store
            .set_local_product_settings(
                command_id,
                fingerprint,
                expected_revision,
                writer_id,
                values,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        settings_projection(&store)
    }

    pub fn merge_remote_product_settings(
        &self,
        command_id: CommandId,
        schema_version: u32,
        writer_version: SettingsWriterVersion,
        values: ProductSettingsValues,
    ) -> Result<ProductSettingsAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.settings_store_and_time()?;
        let fingerprint = settings_fingerprint(
            b"remote",
            writer_version.counter,
            writer_version.writer_id,
            &values,
        )?;
        store
            .merge_remote_product_settings(
                command_id,
                fingerprint,
                schema_version,
                writer_version,
                values,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        settings_projection(&store)
    }
}

impl Pod0Facade {
    fn settings_store(&self) -> Result<pod0_storage::LibraryStore, FacadeOpenError> {
        self.state()
            .store
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)
    }

    fn settings_store_and_time(
        &self,
    ) -> Result<(pod0_storage::LibraryStore, i64), FacadeOpenError> {
        let state = self.state();
        let store = state
            .store
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)?;
        Ok((store, state.now().value))
    }
}

fn settings_projection(
    store: &pod0_storage::LibraryStore,
) -> Result<ProductSettingsAuthorityProjection, FacadeOpenError> {
    Ok(ProductSettingsAuthorityProjection {
        authoritative: store
            .product_settings_is_authoritative()
            .map_err(FacadeOpenError::from)?,
        settings: store.product_settings().map_err(FacadeOpenError::from)?,
    })
}

fn settings_fingerprint(
    kind: &[u8],
    version: u64,
    writer_id: ContentDigest,
    values: &ProductSettingsValues,
) -> Result<ContentDigest, FacadeOpenError> {
    let values = serde_json::to_vec(values).map_err(|_| FacadeOpenError::StorageUnavailable)?;
    let mut hash = Sha256::new();
    hash.update(b"pod0:product-settings:v1\0");
    hash.update(kind);
    hash.update(version.to_be_bytes());
    hash.update(writer_id.into_bytes());
    hash.update(values);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}
