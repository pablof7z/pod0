use std::sync::Arc;

use pod0_application::{
    EpisodeDetailProjection, LibraryProjection, NoteProjectionScope, NotesProjection,
    PlaybackAllowedActions, PlaybackItem, PlaybackProjection, PodcastDetailProjection, Projection,
    ProjectionEnvelope, ProjectionRequest, ProjectionScope, RecallResultProjection, RecallStage,
    UnsupportedProjection,
};
use pod0_domain::CompletionStatus;

use crate::ProjectionSubscriber;
use crate::runtime_state::{FacadeState, failure};

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
                let active = self
                    .listening
                    .playback
                    .active_episode_id
                    .and_then(|episode_id| {
                        self.listening
                            .episodes
                            .iter()
                            .find(|episode| episode.episode_id == episode_id)
                    });
                let has_active = active.is_some();
                let mut value = PlaybackProjection {
                    current: active.map(|episode| PlaybackItem {
                        episode_id: episode.episode_id,
                        title: episode.title.clone(),
                        durable_resume_position_milliseconds: episode
                            .listening
                            .resume_position_milliseconds,
                        meaningful_listening_reached: pod0_domain::meaningful_listening_reached(
                            episode.listening.resume_position_milliseconds,
                        ),
                        segment: self.listening.playback.active_segment,
                        label: self.listening.playback.active_label.clone(),
                        completed: matches!(
                            episode.listening.completion,
                            CompletionStatus::Completed { .. }
                        ),
                        policy_state: self.playback.policy_state,
                        chapter_context: self
                            .playback
                            .chapter
                            .as_ref()
                            .filter(|chapter| chapter.context.episode_id == episode.episode_id)
                            .map(|chapter| chapter.context),
                    }),
                    queue: self.listening.playback.queue.clone(),
                    rate: self.listening.playback.rate,
                    sleep_mode: self.listening.playback.sleep_mode,
                    auto_mark_played_at_natural_end: self
                        .listening
                        .playback
                        .auto_mark_played_at_natural_end,
                    auto_play_next: self.listening.playback.auto_play_next,
                    auto_skip_ads: self.playback.auto_skip_ads,
                    allowed_actions: PlaybackAllowedActions {
                        can_play: has_active && !self.playback.desired_playing,
                        can_pause: has_active && self.playback.desired_playing,
                        can_seek: has_active,
                        can_advance: !self.listening.playback.queue.is_empty(),
                    },
                    host_state: self.playback.host_state,
                    operations: self.operations.clone(),
                };
                value.enforce_bounds(item_limit);
                Projection::Playback { value }
            }
            ProjectionScope::Recall { query_id } => {
                let mut value = if let Some(workflow) = self.recalls.get(&query_id) {
                    RecallResultProjection {
                        query_id,
                        stage: workflow.stage,
                        evidence: workflow.evidence.clone(),
                        failure: workflow.failure.clone(),
                        operation: self
                            .operations
                            .iter()
                            .rev()
                            .find(|operation| operation.command_id == workflow.command_id)
                            .cloned(),
                    }
                } else {
                    RecallResultProjection {
                        query_id,
                        stage: RecallStage::Interrupted,
                        evidence: Vec::new(),
                        failure: Some(failure(pod0_application::CoreFailureCode::NotFound)),
                        operation: None,
                    }
                };
                value.enforce_bounds(item_limit);
                Projection::Recall { value }
            }
            ProjectionScope::EvidenceIndex { episode_id } => Projection::EvidenceIndex {
                value: self.evidence_index_projection(episode_id, offset, item_limit),
            },
            ProjectionScope::Transcript { episode_id, scope } => Projection::Transcript {
                value: self.transcript_projection(
                    episode_id,
                    scope,
                    request.offset,
                    request.max_items,
                ),
            },
            ProjectionScope::Chapter { episode_id, scope } => Projection::Chapter {
                value: self.chapter_projection(
                    episode_id,
                    scope,
                    request.offset,
                    request.max_items,
                ),
            },
            ProjectionScope::Notes { scope } => {
                let mut notes = self.notes.notes.clone();
                match scope {
                    NoteProjectionScope::All => {}
                    NoteProjectionScope::Active => notes.retain(|note| !note.deleted),
                    NoteProjectionScope::Episode { episode_id } => {
                        notes.retain(|note| {
                            !note.deleted
                                && matches!(
                                    note.target,
                                    Some(pod0_domain::NoteTarget::Episode {
                                        episode_id: id,
                                        ..
                                    }) if id == episode_id
                                )
                        });
                        notes.sort_by_key(|note| {
                            let position = match note.target {
                                Some(pod0_domain::NoteTarget::Episode {
                                    position_milliseconds,
                                    ..
                                }) => position_milliseconds,
                                _ => u64::MAX,
                            };
                            (position, note.created_at.value, note.note_id)
                        });
                    }
                    NoteProjectionScope::Unsupported { .. } => notes.clear(),
                }
                let mut value = NotesProjection {
                    scope,
                    collection_revision: self.notes.revision,
                    notes,
                    operations: self.operations.clone(),
                    has_more: false,
                };
                value.enforce_bounds(offset, item_limit);
                Projection::Notes { value }
            }
            ProjectionScope::Clips { scope } => {
                let mut clips = self.clips.clips.clone();
                match scope {
                    pod0_application::ClipProjectionScope::All => {}
                    pod0_application::ClipProjectionScope::Active => {
                        clips.retain(|clip| !clip.deleted);
                    }
                    pod0_application::ClipProjectionScope::Clip { clip_id } => {
                        clips.retain(|clip| clip.clip_id == clip_id);
                    }
                    pod0_application::ClipProjectionScope::Episode { episode_id } => {
                        clips.retain(|clip| !clip.deleted && clip.episode_id == episode_id);
                    }
                    pod0_application::ClipProjectionScope::Unsupported { .. } => clips.clear(),
                }
                let mut value = pod0_application::ClipsProjection {
                    scope,
                    collection_revision: self.clips.revision,
                    clips,
                    operations: self.operations.clone(),
                    has_more: false,
                };
                value.enforce_bounds(offset, item_limit);
                Projection::Clips { value }
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
