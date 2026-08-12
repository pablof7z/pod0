use pod0_application::{Projection, ProjectionRequest};
use pod0_domain::{EpisodeRecord, PodcastRecord, PodcastSubscriptionRecord};

use crate::runtime_state::FacadeState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ProjectionDeliveryContent {
    Library {
        podcasts: Vec<PodcastRecord>,
        subscriptions: Vec<PodcastSubscriptionRecord>,
        episodes: Vec<EpisodeRecord>,
    },
    Bounded(Box<Projection>),
}

impl FacadeState {
    pub(super) fn delivery_content(
        &self,
        request: ProjectionRequest,
        projection: &Projection,
    ) -> ProjectionDeliveryContent {
        if matches!(request.scope, pod0_application::ProjectionScope::Library) {
            return ProjectionDeliveryContent::Library {
                podcasts: self.listening.podcasts.clone(),
                subscriptions: self.listening.subscriptions.clone(),
                episodes: self.listening.episodes.clone(),
            };
        }

        let mut projection = projection.clone();
        clear_global_operations(&mut projection);
        ProjectionDeliveryContent::Bounded(Box::new(projection))
    }
}

fn clear_global_operations(projection: &mut Projection) {
    match projection {
        Projection::Library { value } => value.operations.clear(),
        Projection::PodcastDetail { value } => value.operations.clear(),
        Projection::EpisodeDetail { value } => value.operations.clear(),
        Projection::Playback { value } => value.operations.clear(),
        Projection::Transcript { value } => value.operations.clear(),
        Projection::Chapter { value } => value.operations.clear(),
        Projection::Publications { value } => value.operations.clear(),
        Projection::Notes { value } => value.operations.clear(),
        Projection::Memories { value } => value.operations.clear(),
        Projection::Clips { value } => value.operations.clear(),
        Projection::NewEpisodeNotificationSettings { .. }
        | Projection::RecallConfiguration { .. }
        | Projection::Recall { .. }
        | Projection::EvidenceIndex { .. }
        | Projection::TranscriptWorkflows { .. }
        | Projection::ChapterWorkflows { .. }
        | Projection::Downloads { .. }
        | Projection::ScheduledAgent { .. }
        | Projection::AgentConversations { .. }
        | Projection::AgentConversation { .. }
        | Projection::Unsupported { .. } => {}
    }
}
