//! Podcast per-app FFI surface.
//!
//! `extern "C"` symbols Swift links against:
//!
//! - [`nmp_app_podcast_register`] — wire `nmp-app-template` defaults into
//!   the supplied `NmpApp` and return an opaque handle for subsequent
//!   snapshot / unregister calls.
//! - [`nmp_app_podcast_snapshot`] — serialize the current app state into a
//!   freshly-allocated nul-terminated JSON C string. Swift owns the pointer
//!   until it calls `nmp_app_podcast_snapshot_free`.
//! - [`nmp_app_podcast_snapshot_free`] — companion deallocator for the
//!   snapshot string.
//! - [`nmp_app_podcast_unregister`] — drop the handle and free associated
//!   resources. Idempotent.
//!
//! ## Doctrine
//!
//! * **D0** — `nmp-core` never carries podcast-domain nouns; this crate is
//!   the composition point.
//! * **D6** — every entry point is fire-and-forget. Null pointers, missing
//!   strings, serialization failures, and poisoned mutexes all degrade
//!   silently rather than raising across the FFI.
//! * **No business logic in Swift** — Swift takes the JSON string, decodes
//!   to the appropriate types, and renders. All logic happens in Rust.
//!
//! ## Module layout
//!
//! Split across sub-modules to keep each file under the 500-LOC hard ceiling.
//! Every `pub extern "C"` symbol Swift links against is re-exported below.

pub mod actions;
mod audio_report;
mod data_dir;
mod download_report;
pub(crate) mod handle;
mod helpers;
pub mod projections;
#[cfg(test)]
mod projections_tests;
#[cfg(test)]
mod projections_tests_ext;
mod register;
pub(crate) mod snapshot;
mod snapshot_categories;
mod snapshot_owned;
mod snapshot_queue;
#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod snapshot_tests_ext;
mod voice_report;

pub use actions::{
    AgentActionModule, AgentApproveAction, AgentChatAction, AgentClearConversationAction,
    AgentDenyAction, AgentPicksModule, AgentTasksAction, AgentTasksModule,
    CancelAllDownloadsAction, CancelBriefingAction, CancelDownloadAction, CategorizationAction,
    CategorizationModule, ChaptersAction, ChaptersActionModule, ClipAction, ClipActionModule,
    DownloadEpisodeAction, InboxAction, InboxActionModule, KnowledgeAction, KnowledgeActionModule,
    MemoryAction, MemoryActionModule, NipF4PublishModule, PauseAction, PauseDownloadAction,
    PicksAction, PlayAction, PlayerAction, PlayerActionModule, PodcastAction, PodcastActionModule,
    PublishAction, QueueAction, QueueActionModule, RequestBriefingAction, ResumeDownloadAction,
    ScheduleBriefingAction, SeekAction, SendAgentMessageAction, SetSleepTimerAction, SetSpeedAction,
    SetVoiceAction, SetVolumeAction, SettingsAction, SettingsActionModule, SiriAction,
    SiriActionModule, SiriPlayLatestAction, SiriResumeAction, SpeakAction, StopAction,
    StopVoiceAction, TtsEpisodeAction, TtsEpisodeModule, VoiceAction, VoiceActionModule,
    WikiAction, WikiActionModule,
    ACTION_AGENT_APPROVE, ACTION_AGENT_CLEAR, ACTION_AGENT_DENY, ACTION_AGENT_SEND,
    ACTION_BRIEFING_CANCEL, ACTION_BRIEFING_REQUEST, ACTION_BRIEFING_SCHEDULE,
    ACTION_CLIP_AUTO_SNIP, ACTION_CLIP_CREATE, ACTION_CLIP_DELETE,
    ACTION_INBOX_DISMISS, ACTION_INBOX_MARK_LISTENED, ACTION_INBOX_TRIAGE,
    ACTION_KNOWLEDGE_CLEAR_RESULTS, ACTION_KNOWLEDGE_INDEX_EPISODE, ACTION_KNOWLEDGE_SEARCH,
    ACTION_PLAYER_CANCEL_ALL_DOWNLOADS, ACTION_PLAYER_CANCEL_DOWNLOAD, ACTION_PLAYER_DOWNLOAD,
    ACTION_PLAYER_PAUSE, ACTION_PLAYER_PAUSE_DOWNLOAD, ACTION_PLAYER_PLAY,
    ACTION_PLAYER_RESUME_DOWNLOAD, ACTION_PLAYER_SEEK, ACTION_PLAYER_SET_SLEEP_TIMER,
    ACTION_PLAYER_SET_SPEED, ACTION_PLAYER_SET_VOLUME, ACTION_PLAYER_SKIP_BACKWARD,
    ACTION_PLAYER_SKIP_FORWARD, ACTION_PLAYER_STOP,
    ACTION_PUBLISH_CREATE_OWNED, ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM,
    ACTION_PUBLISH_PUBLISH_EPISODE, ACTION_PUBLISH_PUBLISH_SHOW, ACTION_PUBLISH_REMOVE_OWNED,
    ACTION_SIRI_PLAY_LATEST, ACTION_SIRI_RESUME,
    ACTION_TTS_DELETE, ACTION_TTS_GENERATE, ACTION_TTS_PLAY,
    ACTION_VOICE_ACTIVATE, ACTION_VOICE_DEACTIVATE, ACTION_VOICE_SET_VOICE,
    ACTION_VOICE_SPEAK, ACTION_VOICE_STOP,
    PICKS_LIMIT, PICKS_PER_SHOW_CAP, TTS_NAMESPACE,
};
pub use audio_report::nmp_app_podcast_audio_report;
pub use data_dir::nmp_app_podcast_set_data_dir;
pub use download_report::nmp_app_podcast_download_report;
pub use handle::PodcastHandle;
pub use projections::{
    AccountSummary, AgentMessageSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary,
    BriefingSegmentSummary, BriefingSnapshot, CategoryBrowseItem, ChapterSummary, ClipSummary,
    CommentSummary, ContactSummary, ConversationsSnapshot, DownloadItemSnapshot,
    DownloadQueueSnapshot, EpisodeSummary, InboxItem, KnowledgeSearchResult, MemoryFact,
    NostrShowSummary, OwnedPodcastInfo, PendingApprovalSnapshot, PodcastSummary, SettingsSnapshot,
    SocialSnapshot, TranscriptEntry, TtsEpisodeSummary, VoiceState, WidgetSnapshot, WikiArticle,
};
pub use register::nmp_app_podcast_register;
pub use snapshot::{
    nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free, nmp_app_podcast_snapshot_rev,
    nmp_app_podcast_unregister, PodcastUpdate,
};
pub use voice_report::nmp_app_podcast_voice_report;
