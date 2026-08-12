use std::sync::Arc;

use pod0_application::{Clock, PlaybackPolicyState};
use pod0_domain::StateRevision;
use pod0_recall_index::RecallIndex;
use pod0_storage::{
    AgentStore, EvidenceStore, LibraryStore, PublicationStore, ScheduledAgentStore, TranscriptStore,
};

use crate::runtime_playback_state::PlaybackRuntime;
use crate::runtime_state::FacadeState;

pub(super) struct FacadeStores {
    pub(super) listening: LibraryStore,
    pub(super) evidence: EvidenceStore,
    pub(super) transcript: TranscriptStore,
    pub(super) scheduled_agent: Option<ScheduledAgentStore>,
    pub(super) agent: AgentStore,
    pub(super) publication: PublicationStore,
}

impl FacadeState {
    pub(super) fn open(
        stores: FacadeStores,
        mut recall_index: RecallIndex,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, pod0_storage::StorageError> {
        let FacadeStores {
            listening: store,
            evidence: evidence_store,
            transcript: transcript_store,
            scheduled_agent: scheduled_agent_store,
            agent: agent_store,
            publication: publication_store,
        } = stores;
        let _ = store.clear_session_sleep_timer(clock.now().value)?;
        let _ = store.recover_download_artifacts()?;
        let listening = store.snapshot()?;
        let new_episode_notification_settings = store.new_episode_notification_settings()?;
        let notes = store.note_snapshot()?;
        let memories = store.memory_snapshot()?;
        let clips = store.clip_snapshot()?;
        let recall_configuration = store.recall_configuration()?.unwrap_or_default();
        recall_index
            .activate_embedding_space(recall_configuration.embedding_space_id)
            .map_err(|_| pod0_storage::StorageError::InvalidRecallConfiguration)?;
        let playback = PlaybackRuntime {
            policy_state: if listening.playback.active_episode_id.is_some() {
                PlaybackPolicyState::Paused
            } else {
                PlaybackPolicyState::Idle
            },
            ..PlaybackRuntime::default()
        };
        let mut state = Self {
            clock,
            revision: StateRevision::new(
                listening
                    .playback
                    .revision
                    .value
                    .max(notes.revision.value)
                    .max(memories.revision.value)
                    .max(clips.revision.value),
            ),
            listening,
            new_episode_notification_settings,
            notes,
            memories,
            clips,
            store: Some(store),
            evidence_store: Some(evidence_store),
            transcript_store: Some(transcript_store),
            scheduled_agent_store,
            agent_store: Some(agent_store),
            publication_store: Some(publication_store),
            recall_index,
            recall_configuration,
            playback,
            ..Self::default()
        };
        state.rehydrate_publisher_chapter_workflows()?;
        state.rehydrate_download_workflows()?;
        state.resume_automatic_download_commands();
        state.rehydrate_feed_workflows()?;
        state.rehydrate_feed_discovery_workflows()?;
        state.rehydrate_model_chapter_workflows()?;
        state.rehydrate_transcript_workflows()?;
        state.resume_playback_transcript_commands();
        state.rehydrate_scheduled_agent_workflows()?;
        state.rehydrate_agent_turns()?;
        state.rehydrate_publications()?;
        Ok(state)
    }
}
