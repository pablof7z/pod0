use pod0_domain::{
    CategoryId, CategoryItemKind, CategoryReplacementInput, CategoryRevision, CategorySettings,
    CommandId, PodcastId, StateRevision,
};
use sha2::{Digest as _, Sha256};

use crate::{FacadeOpenError, Pod0Facade};

pub const MAX_CATEGORY_PROJECTION_PODCASTS: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategoryProjection {
    pub category_id: CategoryId,
    pub revision: CategoryRevision,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub color_hex: Option<String>,
    pub origin: pod0_domain::CategoryOrigin,
    pub settings: CategorySettings,
    pub podcast_ids: Vec<PodcastId>,
    pub generated_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategoryAuthorityProjection {
    pub authoritative: bool,
    pub revision: StateRevision,
    pub categories: Vec<CategoryProjection>,
    pub total_podcast_members: u64,
    pub truncated: bool,
}

#[uniffi::export]
impl Pod0Facade {
    pub fn category_authority(&self) -> Result<CategoryAuthorityProjection, FacadeOpenError> {
        category_projection(&self.category_store()?)
    }

    pub fn import_legacy_categories(
        &self,
        command_id: CommandId,
        source_generation: u64,
        categories: Vec<CategoryReplacementInput>,
    ) -> Result<CategoryAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.category_store_and_time()?;
        let fingerprint = fingerprint(b"legacy-import", source_generation, &categories)?;
        store
            .import_legacy_categories(
                command_id,
                &fingerprint,
                source_generation,
                &categories,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        category_projection(&store)
    }

    pub fn replace_categories(
        &self,
        command_id: CommandId,
        expected_revision: StateRevision,
        categories: Vec<CategoryReplacementInput>,
    ) -> Result<CategoryAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.category_store_and_time()?;
        let fingerprint = fingerprint(b"replace", expected_revision.value, &categories)?;
        store
            .replace_categories(
                command_id,
                &fingerprint,
                expected_revision,
                &categories,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        category_projection(&store)
    }

    pub fn move_podcast_to_category(
        &self,
        command_id: CommandId,
        expected_revision: StateRevision,
        podcast_id: PodcastId,
        category_id: CategoryId,
    ) -> Result<CategoryAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.category_store_and_time()?;
        let payload = (podcast_id.into_bytes(), category_id.into_bytes());
        let fingerprint = fingerprint(b"move-podcast", expected_revision.value, &payload)?;
        store
            .move_podcast_to_category(
                command_id,
                &fingerprint,
                expected_revision,
                podcast_id,
                category_id,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        category_projection(&store)
    }

    pub fn set_category_settings(
        &self,
        command_id: CommandId,
        category_id: CategoryId,
        expected_revision: CategoryRevision,
        settings: CategorySettings,
    ) -> Result<CategoryAuthorityProjection, FacadeOpenError> {
        let (store, observed_at_ms) = self.category_store_and_time()?;
        if !store
            .category_store_is_authoritative()
            .map_err(FacadeOpenError::from)?
        {
            return Err(FacadeOpenError::NotAuthoritative);
        }
        let payload = (category_id.into_bytes(), expected_revision.value, settings);
        let fingerprint = fingerprint(b"settings", expected_revision.value, &payload)?;
        store
            .update_category_settings(
                command_id,
                &fingerprint,
                category_id,
                expected_revision,
                settings,
                observed_at_ms,
            )
            .map_err(FacadeOpenError::from)?;
        category_projection(&store)
    }
}

impl Pod0Facade {
    fn category_store(&self) -> Result<pod0_storage::LibraryStore, FacadeOpenError> {
        self.state()
            .store
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)
    }

    fn category_store_and_time(
        &self,
    ) -> Result<(pod0_storage::LibraryStore, i64), FacadeOpenError> {
        let state = self.state();
        Ok((
            state
                .store
                .clone()
                .ok_or(FacadeOpenError::StorageUnavailable)?,
            state.now().value,
        ))
    }
}

fn category_projection(
    store: &pod0_storage::LibraryStore,
) -> Result<CategoryAuthorityProjection, FacadeOpenError> {
    let authoritative = store
        .category_store_is_authoritative()
        .map_err(FacadeOpenError::from)?;
    let snapshot = store.category_snapshot().map_err(FacadeOpenError::from)?;
    let total_podcast_members = snapshot
        .categories
        .iter()
        .map(|category| {
            category
                .members
                .iter()
                .filter(|member| member.kind == CategoryItemKind::Podcast)
                .count()
        })
        .sum::<usize>();
    let mut remaining = MAX_CATEGORY_PROJECTION_PODCASTS;
    let categories = snapshot
        .categories
        .into_iter()
        .map(|category| {
            let podcast_ids = category
                .members
                .into_iter()
                .filter(|member| member.kind == CategoryItemKind::Podcast)
                .take(remaining)
                .map(|member| PodcastId::from_bytes(member.item_id.into_bytes()))
                .collect::<Vec<_>>();
            remaining -= podcast_ids.len();
            CategoryProjection {
                category_id: category.category_id,
                revision: category.revision,
                name: category.name,
                slug: category.slug,
                description: category.description,
                color_hex: category.color_hex,
                origin: category.origin,
                settings: category.settings,
                podcast_ids,
                generated_at: category.created_at,
            }
        })
        .collect();
    Ok(CategoryAuthorityProjection {
        authoritative,
        revision: snapshot.revision,
        categories,
        total_podcast_members: u64::try_from(total_podcast_members).unwrap_or(u64::MAX),
        truncated: total_podcast_members > MAX_CATEGORY_PROJECTION_PODCASTS,
    })
}

fn fingerprint<T: serde::Serialize>(
    kind: &[u8],
    version: u64,
    value: &T,
) -> Result<String, FacadeOpenError> {
    let payload = serde_json::to_vec(value).map_err(|_| FacadeOpenError::StorageUnavailable)?;
    let mut hash = Sha256::new();
    hash.update(b"pod0:categories:v1\0");
    hash.update(kind);
    hash.update(version.to_be_bytes());
    hash.update(payload);
    Ok(format!("{:x}", hash.finalize()))
}
