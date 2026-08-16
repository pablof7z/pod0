use crate::Pod0Facade;
use crate::episode_activity_projection::EpisodeActivityPage;

#[uniffi::export]
impl Pod0Facade {
    pub fn causal_activity_page(
        &self,
        correlation_id: pod0_domain::ActivityCorrelationId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> EpisodeActivityPage {
        activity_page(self, |store| {
            store.activity_page_for_correlation(correlation_id, after_sequence, requested_count)
        })
    }

    pub fn operation_activity_page(
        &self,
        command_id: pod0_domain::CommandId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> EpisodeActivityPage {
        activity_page(self, |store| {
            store.activity_page_for_operation(command_id, after_sequence, requested_count)
        })
    }

    pub fn support_activity_page(
        &self,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> EpisodeActivityPage {
        activity_page(self, |store| {
            store.support_activity_page(after_sequence, requested_count)
        })
    }
}

fn activity_page(
    facade: &Pod0Facade,
    read: impl FnOnce(
        &pod0_storage::LibraryStore,
    ) -> Result<pod0_storage::ActivityPage, pod0_storage::StorageError>,
) -> EpisodeActivityPage {
    let state = facade.state();
    let Some(store) = state.store.as_ref() else {
        return EpisodeActivityPage::unavailable();
    };
    read(store)
        .map(EpisodeActivityPage::from_storage)
        .unwrap_or_else(|_| EpisodeActivityPage::unavailable())
}
