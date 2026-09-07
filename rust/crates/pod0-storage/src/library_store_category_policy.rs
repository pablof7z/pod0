use pod0_domain::{
    AutoDownloadPolicy, CategoryAutoDownloadSource, CategoryId, CategoryItemKind, CategoryRevision,
    CategorySettings, CommandId, LibraryItemId, PodcastId, ResolvedCategoryAutoDownloadPolicy,
    StateRevision,
};
use rusqlite::{Transaction, params};

use crate::category_store_model::encode_settings;
use crate::category_store_write_support::{bump_category, finish_category_command};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn update_category_settings(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        category_id: CategoryId,
        expected_revision: CategoryRevision,
        settings: CategorySettings,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_category_settings(
            self.path(),
            command_id,
            command_fingerprint,
            category_id,
            expected_revision,
            settings,
            observed_at_ms,
        )
    }

    pub fn effective_category_auto_download(
        &self,
        podcast_id: PodcastId,
        subscription_policy: AutoDownloadPolicy,
    ) -> Result<ResolvedCategoryAutoDownloadPolicy, StorageError> {
        let item_id = LibraryItemId::from_bytes(podcast_id.into_bytes());
        let snapshot = self.category_snapshot()?;
        let mut candidates = snapshot
            .categories
            .into_iter()
            .filter_map(|category| {
                let policy = category.settings.auto_download_override?;
                let added_at = category
                    .members
                    .iter()
                    .find(|member| {
                        member.item_id == item_id && member.kind == CategoryItemKind::Podcast
                    })?
                    .added_at;
                Some((added_at, category.category_id, policy))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .0
                .value
                .cmp(&left.0.value)
                .then_with(|| left.1.into_bytes().cmp(&right.1.into_bytes()))
        });
        let Some((_, selected, policy)) = candidates.first().copied() else {
            return Ok(ResolvedCategoryAutoDownloadPolicy {
                policy: subscription_policy,
                source: CategoryAutoDownloadSource::Subscription,
                conflicting_category_ids: Vec::new(),
            });
        };
        let conflicting_category_ids = candidates
            .into_iter()
            .skip(1)
            .filter_map(|(_, category_id, candidate)| (candidate != policy).then_some(category_id))
            .collect();
        Ok(ResolvedCategoryAutoDownloadPolicy {
            policy,
            source: CategoryAutoDownloadSource::Category {
                category_id: selected,
            },
            conflicting_category_ids,
        })
    }
}

pub(crate) fn update_settings_in_transaction(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    category_id: CategoryId,
    settings: CategorySettings,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let (code, latest, wifi_only, rag, notifications) = encode_settings(settings)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_category_settings SET auto_download_code=?2,\
             auto_download_latest_count=?3,wifi_only=?4,rag_enabled=?5,\
             notifications_enabled=?6,updated_at_ms=?7 WHERE category_id=?1",
            params![
                category_id.into_bytes().as_slice(),
                code,
                latest,
                wifi_only,
                rag,
                notifications,
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("update category settings", error))?;
    if changed != 1 {
        return Err(StorageError::EntityNotFound);
    }
    bump_category(transaction, category_id, observed_at_ms)?;
    finish_category_command(transaction, command_id, fingerprint, observed_at_ms)
}
