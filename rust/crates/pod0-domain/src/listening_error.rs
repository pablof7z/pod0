#[derive(Debug, uniffi::Error)]
pub enum ListeningDomainError {
    InvalidFeedUrl,
    InvalidFeedComparisonKey,
    AmbiguousPodcastFeedIdentity,
    PodcastIdConflict,
    MissingLegacyParentIdentity,
    EmptyPublisherGuid,
    AmbiguousEpisodeIdentity,
    EpisodeIdConflict,
    DuplicatePodcastId,
    DuplicateSubscription,
    DuplicateEpisodeId,
    DuplicateQueueEntryId,
    MissingPodcast,
    MissingEpisode,
    InvalidLatestCount,
    InvalidPlaybackRate,
    InvalidArtifactReference,
    InvalidSegmentBounds,
    InvalidSleepDuration,
    CompletedEpisodeHasResumePosition,
}

impl std::fmt::Display for ListeningDomainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFeedUrl => "feed URL must be an absolute HTTP(S) URL without whitespace",
            Self::InvalidFeedComparisonKey => "feed comparison key does not match v1 policy",
            Self::AmbiguousPodcastFeedIdentity => "multiple podcast IDs match one feed identity",
            Self::PodcastIdConflict => "podcast ID is already attached to another feed identity",
            Self::MissingLegacyParentIdentity => {
                "record has neither modern nor legacy parent identity"
            }
            Self::EmptyPublisherGuid => "episode publisher GUID must not be empty",
            Self::AmbiguousEpisodeIdentity => "multiple episode IDs match one podcast and GUID",
            Self::EpisodeIdConflict => {
                "episode ID is already attached to another external identity"
            }
            Self::DuplicatePodcastId => "listening snapshot contains a duplicate podcast ID",
            Self::DuplicateSubscription => "listening snapshot contains a duplicate subscription",
            Self::DuplicateEpisodeId => "listening snapshot contains a duplicate episode ID",
            Self::DuplicateQueueEntryId => "listening snapshot contains a duplicate queue entry ID",
            Self::MissingPodcast => "listening snapshot references a missing podcast",
            Self::MissingEpisode => "listening snapshot references a missing episode",
            Self::InvalidLatestCount => "latest-N auto-download count must be positive",
            Self::InvalidPlaybackRate => "playback rate must be between 0.5x and 3.0x",
            Self::InvalidArtifactReference => {
                "artifact reference must have a version and opaque key"
            }
            Self::InvalidSegmentBounds => "queue segment end must be greater than its start",
            Self::InvalidSleepDuration => "sleep duration must be positive",
            Self::CompletedEpisodeHasResumePosition => {
                "completed episode must have zero resume position"
            }
        })
    }
}

impl std::error::Error for ListeningDomainError {}
