use crate::{ActivityDomain, ActivityOwner, HostObservation};

#[must_use]
pub const fn host_observation_owner(observation: &HostObservation) -> ActivityOwner {
    use ActivityDomain as Domain;
    use HostObservation as Observation;
    match observation {
        Observation::AuthorizedEffectCancellationApplied { .. } => ActivityOwner::CorrelatedEffect,
        Observation::FeedBytesFetched { .. }
        | Observation::FeedNotModified { .. }
        | Observation::NewEpisodeNotificationDelivered { .. } => {
            ActivityOwner::Domain(Domain::LibraryFeed)
        }
        Observation::LibraryDocumentFetched { .. } => ActivityOwner::Domain(Domain::LibraryFeed),
        Observation::PlaybackObserved { .. } => ActivityOwner::Domain(Domain::Playback),
        Observation::RecallQueryEmbedded { .. }
        | Observation::RecallSpansEmbedded { .. }
        | Observation::RecallCandidatesReranked { .. }
        | Observation::LegacyRecallIndexArtifactsRemoved { .. } => {
            ActivityOwner::Domain(Domain::RecallKnowledge)
        }
        Observation::PublisherChaptersFetched { .. }
        | Observation::ChapterModelProviderAccepted { .. }
        | Observation::ChapterModelCompleted { .. }
        | Observation::ChapterModelFailed { .. } => ActivityOwner::Domain(Domain::Chapter),
        Observation::DownloadAccepted { .. }
        | Observation::DownloadStaged { .. }
        | Observation::DownloadCancelled { .. }
        | Observation::DownloadArtifactRemoved { .. } => ActivityOwner::Domain(Domain::Download),
        Observation::TranscriptCapabilityObserved { .. } => {
            ActivityOwner::Domain(Domain::Transcript)
        }
        Observation::ScheduledAgentExecutionObserved { .. } => {
            ActivityOwner::Domain(Domain::ScheduledAgent)
        }
        Observation::AgentModelCompleted { .. }
        | Observation::AgentApprovalObserved { .. }
        | Observation::AgentCapabilityObserved { .. } => {
            ActivityOwner::Domain(Domain::AgentPublication)
        }
        Observation::CoreWakeReached { .. } => ActivityOwner::Domain(Domain::Lifecycle),
        Observation::Failed { .. } | Observation::Cancelled => ActivityOwner::CorrelatedEffect,
        Observation::Unsupported { .. } => ActivityOwner::Boundary,
    }
}
