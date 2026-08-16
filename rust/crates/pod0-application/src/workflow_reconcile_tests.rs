use pod0_domain::{
    ArtifactReference, AutoDownloadMode, AutoDownloadPolicy, CompletionStatus, ContentDigest,
    DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeId, EpisodeListeningState, EpisodeRecord,
    ListeningDomainSnapshot, ListeningPlaybackPolicy, PlaybackRatePermille, PlaybackSleepMode,
    PodcastId, PodcastSubscriptionRecord, StateRevision, TranscriptArtifactStatus,
    TranscriptSource, TranscriptStartPolicy, UnixTimestampMilliseconds,
};

use crate::{
    LocalAudioCapability, TranscriptCredentialCapabilities, TranscriptProvider,
    WORKFLOW_CONFIGURATION_SCHEMA_VERSION, WorkflowCapabilitySnapshot, WorkflowConfiguration,
    WorkflowConfigurationInput, WorkflowConfigurationOrigin, WorkflowReconcileIntent,
    plan_workflow_reconciliation,
};

#[test]
fn planner_derives_policy_from_rust_configuration_and_subscription_state() {
    let automatic = PodcastId::from_bytes([1; 16]);
    let played_only = PodcastId::from_bytes([2; 16]);
    let first = EpisodeId::from_bytes([3; 16]);
    let second = EpisodeId::from_bytes([4; 16]);
    let listening = snapshot(
        [
            (automatic, TranscriptStartPolicy::Automatic),
            (played_only, TranscriptStartPolicy::WhenPlayed),
        ],
        [
            episode(first, automatic, true, true),
            episode(second, played_only, true, true),
        ],
    );
    let configuration = configuration();
    let capabilities = capabilities(first);

    let first_plan = plan_workflow_reconciliation(&listening, &configuration, &capabilities);
    let replay = plan_workflow_reconciliation(&listening, &configuration, &capabilities);

    assert_eq!(first_plan, replay);
    assert!(first_plan.intents.iter().any(|intent| matches!(intent,
        WorkflowReconcileIntent::EnsureTranscript { episode_id, configuration, .. }
            if *episode_id == first
                && configuration.model == "universal-3-pro"
                && configuration.local_audio_url.as_deref() == Some("file:///audio.m4a")
                && configuration.credential_available
    )));
    assert!(!first_plan.intents.iter().any(|intent| matches!(intent,
        WorkflowReconcileIntent::EnsureTranscript { episode_id, .. } if *episode_id == second
    )));
    assert_eq!(
        first_plan
            .intents
            .iter()
            .filter(|intent| matches!(
                intent,
                WorkflowReconcileIntent::EnsurePublisherChapters { .. }
            ))
            .count(),
        2
    );
    assert_eq!(
        first_plan
            .intents
            .iter()
            .filter(|intent| matches!(intent, WorkflowReconcileIntent::EnsureModelChapters { .. }))
            .count(),
        2
    );
    assert!(matches!(
        first_plan.intents.last(),
        Some(WorkflowReconcileIntent::ReconcileScheduledRuns)
    ));
}

#[test]
fn invalid_configuration_fails_closed_without_partial_intents() {
    let mut configuration = configuration();
    configuration.value.chapter_model = "  ".to_owned();
    let listening = snapshot([], []);
    assert!(
        plan_workflow_reconciliation(
            &listening,
            &configuration,
            &capabilities(EpisodeId::from_bytes([1; 16]))
        )
        .intents
        .is_empty()
    );
}

fn configuration() -> WorkflowConfiguration {
    WorkflowConfiguration {
        schema_version: WORKFLOW_CONFIGURATION_SCHEMA_VERSION,
        revision: StateRevision::new(7),
        origin: WorkflowConfigurationOrigin::User,
        value: WorkflowConfigurationInput {
            transcript_provider: TranscriptProvider::AssemblyAi,
            eleven_labs_model: "scribe_v1".to_owned(),
            assembly_ai_model: "universal-3-pro".to_owned(),
            open_router_model: "openai/whisper-1".to_owned(),
            auto_publisher_transcripts: true,
            auto_provider_transcripts: true,
            chapter_model: "openai/gpt-4o-mini".to_owned(),
        },
    }
}

fn capabilities(episode_id: EpisodeId) -> WorkflowCapabilitySnapshot {
    WorkflowCapabilitySnapshot {
        snapshot_id: ContentDigest::from_bytes([9; 32]),
        observed_at: UnixTimestampMilliseconds::new(20),
        credentials: TranscriptCredentialCapabilities {
            eleven_labs: false,
            assembly_ai: true,
            open_router: false,
            apple_speech: true,
        },
        local_audio: vec![LocalAudioCapability {
            episode_id,
            local_audio_url: "file:///audio.m4a".to_owned(),
        }],
    }
}

fn snapshot(
    subscriptions: impl IntoIterator<Item = (PodcastId, TranscriptStartPolicy)>,
    episodes: impl IntoIterator<Item = EpisodeRecord>,
) -> ListeningDomainSnapshot {
    ListeningDomainSnapshot {
        podcasts: vec![],
        subscriptions: subscriptions
            .into_iter()
            .map(
                |(podcast_id, transcript_start_policy)| PodcastSubscriptionRecord {
                    podcast_id,
                    subscribed_at: UnixTimestampMilliseconds::new(1),
                    auto_download: AutoDownloadPolicy {
                        mode: AutoDownloadMode::Off,
                        wifi_only: false,
                    },
                    notifications_enabled: false,
                    default_playback_rate: None,
                    transcript_start_policy,
                },
            )
            .collect(),
        episodes: episodes.into_iter().collect(),
        playback: ListeningPlaybackPolicy {
            active_episode_id: None,
            active_segment: None,
            active_label: None,
            queue: vec![],
            rate: PlaybackRatePermille { value: 1_000 },
            sleep_mode: PlaybackSleepMode::Off,
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
            revision: StateRevision::INITIAL,
        },
    }
}

fn episode(
    episode_id: EpisodeId,
    podcast_id: PodcastId,
    chapters: bool,
    transcript: bool,
) -> EpisodeRecord {
    EpisodeRecord {
        episode_id,
        podcast_id,
        publisher_guid: "guid".to_owned(),
        title: "Episode".to_owned(),
        description: String::new(),
        published_at: UnixTimestampMilliseconds::new(1),
        duration_milliseconds: Some(1_000),
        enclosure_url: "https://x.test/a.mp3".to_owned(),
        enclosure_mime_type: Some("audio/mpeg".to_owned()),
        image_url: None,
        feed_metadata: EpisodeFeedMetadata {
            publisher_transcript: None,
            chapters_url: chapters.then(|| "https://x.test/c.json".to_owned()),
            persons: vec![],
            sound_bites: vec![],
        },
        listening: EpisodeListeningState {
            resume_position_milliseconds: 0,
            completion: CompletionStatus::InProgress,
        },
        is_starred: false,
        download: DownloadArtifactStatus::Unavailable,
        transcript: if transcript {
            TranscriptArtifactStatus::Available {
                reference: ArtifactReference {
                    schema_version: 1,
                    opaque_key: "t".to_owned(),
                },
                source: TranscriptSource::Publisher,
            }
        } else {
            TranscriptArtifactStatus::Unavailable
        },
        generated_audio: None,
    }
}
