use crate::{
    LibraryProjection, MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, PodcastDetailProjection,
};

impl LibraryProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let item_limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let counts = (
            self.podcasts.len(),
            self.subscriptions.len(),
            self.episodes.len(),
        );
        self.podcasts = page(std::mem::take(&mut self.podcasts), offset, item_limit);
        self.subscriptions = page(std::mem::take(&mut self.subscriptions), offset, item_limit);
        self.episodes = page(std::mem::take(&mut self.episodes), offset, item_limit);
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= counts.0 > offset.saturating_add(self.podcasts.len())
            || counts.1 > offset.saturating_add(self.subscriptions.len())
            || counts.2 > offset.saturating_add(self.episodes.len());
    }
}

impl PodcastDetailProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let item_limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let count = self.episodes.len();
        self.episodes = page(std::mem::take(&mut self.episodes), offset, item_limit);
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= count > offset.saturating_add(self.episodes.len());
    }
}

fn page<T>(values: Vec<T>, offset: usize, count: usize) -> Vec<T> {
    values.into_iter().skip(offset).take(count).collect()
}
