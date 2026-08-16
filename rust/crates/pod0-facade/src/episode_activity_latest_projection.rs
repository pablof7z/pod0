use crate::Pod0Facade;
use crate::episode_activity_projection::{EpisodeActivityEntry, project};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LatestEpisodeActivityPage {
    pub available: bool,
    pub items: Vec<EpisodeActivityEntry>,
    pub snapshot_through_sequence: Option<u64>,
    pub next_before_sequence: Option<u64>,
}

#[uniffi::export]
impl Pod0Facade {
    pub fn latest_episode_activity_page(
        &self,
        episode_id: pod0_domain::EpisodeId,
        snapshot_through_sequence: Option<u64>,
        before_sequence: Option<u64>,
        requested_count: u16,
    ) -> LatestEpisodeActivityPage {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LatestEpisodeActivityPage::unavailable();
        };
        store
            .latest_activity_page_for_episode(
                episode_id,
                snapshot_through_sequence,
                before_sequence,
                requested_count,
            )
            .map(LatestEpisodeActivityPage::from_storage)
            .unwrap_or_else(|_| LatestEpisodeActivityPage::unavailable())
    }
}

impl LatestEpisodeActivityPage {
    fn unavailable() -> Self {
        Self {
            available: false,
            items: Vec::new(),
            snapshot_through_sequence: None,
            next_before_sequence: None,
        }
    }

    fn from_storage(page: pod0_storage::LatestActivityPage) -> Self {
        Self {
            available: true,
            items: page.items.into_iter().map(project).collect(),
            snapshot_through_sequence: page.snapshot_through_sequence,
            next_before_sequence: page.next_before_sequence,
        }
    }
}
