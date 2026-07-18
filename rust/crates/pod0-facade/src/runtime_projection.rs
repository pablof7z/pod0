use std::sync::Arc;

use pod0_application::{
    EpisodeDetailProjection, LibraryProjection, PlaybackProjection, PodcastDetailProjection,
    Projection, ProjectionEnvelope, ProjectionRequest, ProjectionScope, UnsupportedProjection,
};

use crate::ProjectionSubscriber;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        let item_limit = request.bounded_max_items();
        let offset = request.bounded_offset();
        let projection = match request.scope {
            ProjectionScope::Library => {
                let mut value = LibraryProjection {
                    podcasts: self.listening.podcasts.clone(),
                    subscriptions: self.listening.subscriptions.clone(),
                    episodes: self.listening.episodes.clone(),
                    operations: self.operations.clone(),
                    has_more: false,
                };
                value.enforce_bounds(offset, item_limit);
                Projection::Library { value }
            }
            ProjectionScope::PodcastDetail { podcast_id } => {
                let mut value = PodcastDetailProjection {
                    podcast: self
                        .listening
                        .podcasts
                        .iter()
                        .find(|podcast| podcast.podcast_id == podcast_id)
                        .cloned(),
                    subscription: self
                        .listening
                        .subscriptions
                        .iter()
                        .find(|subscription| subscription.podcast_id == podcast_id)
                        .cloned(),
                    episodes: self
                        .listening
                        .episodes
                        .iter()
                        .filter(|episode| episode.podcast_id == podcast_id)
                        .cloned()
                        .collect(),
                    operations: self.operations.clone(),
                    has_more: false,
                };
                value
                    .episodes
                    .sort_by_key(|episode| std::cmp::Reverse(episode.published_at.value));
                value.enforce_bounds(offset, item_limit);
                Projection::PodcastDetail { value }
            }
            ProjectionScope::EpisodeDetail { episode_id } => {
                let episode = self
                    .listening
                    .episodes
                    .iter()
                    .find(|episode| episode.episode_id == episode_id)
                    .cloned();
                let podcast_id = episode.as_ref().map(|episode| episode.podcast_id);
                Projection::EpisodeDetail {
                    value: EpisodeDetailProjection {
                        episode,
                        podcast: podcast_id.and_then(|id| {
                            self.listening
                                .podcasts
                                .iter()
                                .find(|podcast| podcast.podcast_id == id)
                                .cloned()
                        }),
                        subscription: podcast_id.and_then(|id| {
                            self.listening
                                .subscriptions
                                .iter()
                                .find(|subscription| subscription.podcast_id == id)
                                .cloned()
                        }),
                        operations: self.operations.clone(),
                    },
                }
            }
            ProjectionScope::Playback => {
                let mut value = PlaybackProjection {
                    current: None,
                    queue: self
                        .listening
                        .playback
                        .queue
                        .iter()
                        .map(|entry| entry.episode_id)
                        .collect(),
                    operations: self.operations.clone(),
                };
                value.enforce_bounds(item_limit);
                Projection::Playback { value }
            }
            ProjectionScope::Unsupported { wire_code } => Projection::Unsupported {
                value: UnsupportedProjection {
                    wire_code,
                    message: "unsupported projection scope".to_owned(),
                },
            },
        };
        ProjectionEnvelope {
            contract_version: pod0_application::FACADE_CONTRACT_VERSION,
            state_revision: self.revision,
            projection,
        }
    }

    pub(super) fn deliveries(&self) -> Vec<(Arc<dyn ProjectionSubscriber>, ProjectionEnvelope)> {
        self.subscribers
            .iter()
            .filter_map(|(id, subscriber)| {
                self.subscriptions
                    .request(*id)
                    .map(|request| (Arc::clone(subscriber), self.snapshot(request)))
            })
            .collect()
    }
}
