use pod0_domain::{
    ListeningDomainSnapshot, ListeningPlaybackPolicy, PlaybackRatePermille, PlaybackSleepMode,
    StateRevision,
};
use pod0_recall_index::{RECALL_INDEX_DIMENSIONS, RecallIndex};

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
