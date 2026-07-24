use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use pod0_application::{CommandLedger, HostRequestLedger, SubscriptionRegistry};
use pod0_domain::{
    ListeningDomainSnapshot, ListeningPlaybackPolicy, PlaybackRatePermille, PlaybackSleepMode,
    StateRevision,
};
use pod0_recall_index::{RECALL_INDEX_DIMENSIONS, RecallIndex};

use crate::runtime_clock::SystemClock;
use crate::runtime_playback_state::PlaybackRuntime;
use crate::runtime_state::FacadeState;

impl Default for FacadeState {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            revision: StateRevision::INITIAL,
            listening: empty_listening_snapshot(),
            new_episode_notification_settings:
                pod0_application::NewEpisodeNotificationSettingsProjection {
                    enabled: true,
                    revision: StateRevision::INITIAL,
                },
            notes: pod0_storage::NoteCollectionSnapshot {
                revision: StateRevision::INITIAL,
                notes: Vec::new(),
            },
            memories: pod0_storage::MemoryCollectionSnapshot {
                revision: StateRevision::INITIAL,
                memories: Vec::new(),
                compiled: None,
            },
            clips: pod0_storage::ClipCollectionSnapshot {
                revision: StateRevision::INITIAL,
                clips: Vec::new(),
            },
            store: None,
            evidence_store: None,
            transcript_store: None,
            scheduled_agent_store: None,
            agent_store: None,
            publication_store: None,
            signer_store: None,
            signer_account: None,
            pending_signers: BTreeMap::new(),
            pending_signer_observations: BTreeMap::new(),
            signer_waiters: BTreeMap::new(),
            pending_publications: VecDeque::new(),
            recall_index: default_recall_index(),
            recall_configuration: pod0_domain::RecallConfiguration::default(),
            recall_interrupts: Arc::default(),
            commands: CommandLedger::default(),
            host_requests: HostRequestLedger::default(),
            host_queue: VecDeque::new(),
            host_cancellations: VecDeque::new(),
            pending_feeds: BTreeMap::new(),
            pending_publisher_chapters: BTreeMap::new(),
            pending_publisher_observations: BTreeMap::new(),
            pending_downloads: BTreeMap::new(),
            pending_download_observations: BTreeMap::new(),
            pending_feed_discovery_notifications: BTreeMap::new(),
            pending_feed_discovery_notification_observations: BTreeMap::new(),
            pending_model_chapters: BTreeMap::new(),
            pending_model_observations: BTreeMap::new(),
            pending_transcripts: BTreeMap::new(),
            pending_transcript_observations: BTreeMap::new(),
            pending_scheduled_agents: BTreeMap::new(),
            pending_scheduled_agent_observations: BTreeMap::new(),
            pending_agents: BTreeMap::new(),
            pending_agent_observations: BTreeMap::new(),
            pending_agent_recalls: BTreeMap::new(),
            pending_agent_recall_observations: BTreeMap::new(),
            pending_core_wakes: BTreeMap::new(),
            pending_evidence_indexes: BTreeMap::new(),
            pending_recall_cutovers: BTreeMap::new(),
            pending_recalls: BTreeMap::new(),
            recalls: BTreeMap::new(),
            playback: PlaybackRuntime::default(),
            operations: Vec::new(),
            subscriptions: SubscriptionRegistry::default(),
            subscribers: BTreeMap::new(),
        }
    }
}

pub(super) fn default_recall_index() -> RecallIndex {
    let mut index = RecallIndex::in_memory(RECALL_INDEX_DIMENSIONS)
        .expect("in-memory recall index must initialize");
    index
        .activate_embedding_space(pod0_domain::RecallConfiguration::default().embedding_space_id)
        .expect("the default embedding space must initialize");
    index
}

pub(super) fn empty_listening_snapshot() -> ListeningDomainSnapshot {
    ListeningDomainSnapshot {
        podcasts: Vec::new(),
        subscriptions: Vec::new(),
        episodes: Vec::new(),
        playback: ListeningPlaybackPolicy {
            active_episode_id: None,
            active_segment: None,
            active_label: None,
            queue: Vec::new(),
            rate: PlaybackRatePermille { value: 1_000 },
            sleep_mode: PlaybackSleepMode::Off,
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
            revision: StateRevision::INITIAL,
        },
    }
}
