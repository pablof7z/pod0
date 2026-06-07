use nmp_app_podcast::ffi::projections::AgentNoteSummary;
use nmp_app_podcast::ffi::{
    AccountSummary, AgentMessageSummary, AgentPickSummary, AgentTaskSummary, AppRelayRow,
    CategoryBrowseItem, ClipSummary, CommentSummary, ContactSummary, MemoryFact, SettingsSnapshot,
    WikiArticle,
};

pub use crate::agent_state::AgentSection;
pub use crate::navigation::{Mode, Pane, Tab};
use crate::provider_model_catalog::ProviderCatalogModel;
use crate::provider_settings_catalog::ProviderSettingItem;
pub use crate::rows::{DownloadRow, EpisodeRow, InboxRow, PodcastRow, SearchResult};
pub use crate::settings_state::SettingsSection;

#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub episode_id: String,
    pub podcast_title: String,
    pub episode_title: String,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub is_playing: bool,
    pub speed: f32,
    pub volume: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub message: String,
    pub ttl_ticks: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub focused: Pane,
    pub tab: Tab,
    pub mode: Mode,
    pub show_help: bool,
    pub update_count: u64,
    pub motion_tick: u64,
    pub library: Vec<PodcastRow>,
    pub episodes: Vec<EpisodeRow>,
    pub selected_podcast: usize,
    pub selected_episode: usize,
    pub now_playing: Option<NowPlaying>,
    pub queue: Vec<EpisodeRow>,
    pub selected_queue: usize,
    pub bookmarks: Vec<EpisodeRow>,
    pub selected_bookmark: usize,
    pub search_results: Vec<SearchResult>,
    pub selected_search: usize,
    pub search_input: String,
    pub subscribe_input: String,
    pub agent_input: String,
    pub agent_memory_input: String,
    pub agent_task_input: String,
    pub agent_note_input: String,
    pub episode_comment_input: String,
    pub inbox: Vec<InboxRow>,
    pub selected_inbox: usize,
    pub clips: Vec<ClipSummary>,
    pub selected_clip: usize,
    pub wiki_articles: Vec<WikiArticle>,
    pub wiki_search_results: Vec<WikiArticle>,
    pub selected_wiki: usize,
    pub agent_section: AgentSection,
    pub agent_messages: Vec<AgentMessageSummary>,
    pub agent_is_busy: bool,
    pub agent_picks: Vec<AgentPickSummary>,
    pub selected_agent_pick: usize,
    pub agent_tasks: Vec<AgentTaskSummary>,
    pub selected_agent_task: usize,
    pub agent_notes: Vec<AgentNoteSummary>,
    pub selected_agent_note: usize,
    pub memory_facts: Vec<MemoryFact>,
    pub selected_memory_fact: usize,
    pub comments: Vec<CommentSummary>,
    pub comments_episode_id: Option<String>,
    pub categories: Vec<CategoryBrowseItem>,
    pub active_account: Option<AccountSummary>,
    pub social_contacts: Vec<ContactSummary>,
    pub social_following_count: usize,
    pub configured_relays: Vec<AppRelayRow>,
    pub inbox_triage_in_progress: bool,
    pub settings: SettingsSnapshot,
    pub settings_section: SettingsSection,
    pub selected_setting: usize,
    pub selected_provider_setting: usize,
    pub selected_relay: usize,
    pub settings_input: String,
    pub relay_input: String,
    pub(crate) provider_catalog_models: Vec<ProviderCatalogModel>,
    pub(crate) provider_catalog_query: String,
    pub(crate) selected_provider_catalog_model: usize,
    pub(crate) provider_catalog_target: Option<ProviderSettingItem>,
    pub status: String,
    pub toasts: Vec<Toast>,
    pub downloads: Vec<DownloadRow>,
    pub selected_download: usize,
    pub playback_error: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            focused: Pane::Library,
            tab: Tab::Library,
            mode: Mode::Normal,
            show_help: false,
            update_count: 0,
            motion_tick: 0,
            library: Vec::new(),
            episodes: Vec::new(),
            selected_podcast: 0,
            selected_episode: 0,
            now_playing: None,
            queue: Vec::new(),
            selected_queue: 0,
            bookmarks: Vec::new(),
            selected_bookmark: 0,
            search_results: Vec::new(),
            selected_search: 0,
            search_input: String::new(),
            subscribe_input: String::new(),
            agent_input: String::new(),
            agent_memory_input: String::new(),
            agent_task_input: String::new(),
            agent_note_input: String::new(),
            episode_comment_input: String::new(),
            inbox: Vec::new(),
            selected_inbox: 0,
            clips: Vec::new(),
            selected_clip: 0,
            wiki_articles: Vec::new(),
            wiki_search_results: Vec::new(),
            selected_wiki: 0,
            agent_section: AgentSection::Chat,
            agent_messages: Vec::new(),
            agent_is_busy: false,
            agent_picks: Vec::new(),
            selected_agent_pick: 0,
            agent_tasks: Vec::new(),
            selected_agent_task: 0,
            agent_notes: Vec::new(),
            selected_agent_note: 0,
            memory_facts: Vec::new(),
            selected_memory_fact: 0,
            comments: Vec::new(),
            comments_episode_id: None,
            categories: Vec::new(),
            active_account: None,
            social_contacts: Vec::new(),
            social_following_count: 0,
            configured_relays: Vec::new(),
            inbox_triage_in_progress: false,
            settings: SettingsSnapshot::default(),
            settings_section: SettingsSection::General,
            selected_setting: 0,
            selected_provider_setting: 0,
            selected_relay: 0,
            settings_input: String::new(),
            relay_input: String::new(),
            provider_catalog_models: Vec::new(),
            provider_catalog_query: String::new(),
            selected_provider_catalog_model: 0,
            provider_catalog_target: None,
            status: "starting kernel".to_string(),
            downloads: Vec::new(),
            selected_download: 0,
            toasts: Vec::new(),
            playback_error: None,
        }
    }
}

impl AppState {
    pub fn download_status_line(&self) -> Option<String> {
        let active = self
            .downloads
            .iter()
            .filter(|d| d.state == "active" || d.state == "queued")
            .collect::<Vec<_>>();
        if active.is_empty() {
            return None;
        }
        let active_progress = self
            .downloads
            .iter()
            .filter(|d| d.state == "active")
            .map(|d| d.progress)
            .collect::<Vec<_>>();
        let avg_progress =
            active_progress.iter().sum::<f32>() / active_progress.len().max(1) as f32;
        Some(format!("↓ {}  {:.0}%", active.len(), avg_progress * 100.0))
    }

    pub fn selected_episode_id(&self) -> Option<String> {
        self.episodes
            .get(self.selected_episode)
            .map(|e| e.id.clone())
    }

    pub fn selected_queue_episode_id(&self) -> Option<String> {
        self.queue.get(self.selected_queue).map(|e| e.id.clone())
    }

    pub fn selected_bookmark_episode_id(&self) -> Option<String> {
        self.bookmarks
            .get(self.selected_bookmark)
            .map(|e| e.id.clone())
    }

    pub fn selected_inbox_episode_id(&self) -> Option<String> {
        self.inbox
            .get(self.selected_inbox)
            .map(|r| r.episode_id.clone())
    }

    pub fn selected_clip_id(&self) -> Option<String> {
        self.clips
            .get(self.selected_clip)
            .map(|clip| clip.id.clone())
    }

    pub fn selected_clip_play_target(&self) -> Option<(String, f64)> {
        self.clips
            .get(self.selected_clip)
            .map(|clip| (clip.episode_id.clone(), clip.start_secs))
    }

    pub fn selected_episode_clip_target(&self) -> Option<(String, f64)> {
        let episode = self.episodes.get(self.selected_episode)?;
        let position = self
            .now_playing
            .as_ref()
            .filter(|np| np.episode_id == episode.id)
            .map(|np| np.position_secs)
            .or(episode.playback_position_secs)
            .unwrap_or(0.0);
        Some((episode.id.clone(), position))
    }

    pub fn now_playing_clip_target(&self) -> Option<(String, f64)> {
        self.now_playing
            .as_ref()
            .map(|np| (np.episode_id.clone(), np.position_secs))
    }

    pub fn selected_search_feed_url(&self) -> Option<String> {
        self.search_results
            .get(self.selected_search)
            .and_then(|r| r.feed_url.clone())
    }

    pub fn selected_wiki_id(&self) -> Option<String> {
        self.wiki_articles
            .get(self.selected_wiki)
            .map(|w| w.id.clone())
    }

    pub fn push_toast(&mut self, msg: &str) {
        self.toasts.push(Toast {
            message: msg.to_string(),
            ttl_ticks: 20,
        });
    }

    pub fn tick_toasts(&mut self) {
        for toast in &mut self.toasts {
            toast.ttl_ticks = toast.ttl_ticks.saturating_sub(1);
        }
        self.toasts.retain(|t| t.ttl_ticks > 0);
    }

    pub fn tick_motion(&mut self) {
        self.motion_tick = self.motion_tick.wrapping_add(1);
    }

    pub fn focus(&mut self, pane: Pane) {
        self.focused = pane;
    }

    pub fn next_tab(&mut self) {
        self.tab = self.tab.next();
    }

    pub fn previous_tab(&mut self) {
        self.tab = self.tab.previous();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn close_help(&mut self) -> bool {
        let was_open = self.show_help;
        self.show_help = false;
        was_open
    }

    pub fn next_podcast(&mut self) {
        if !self.library.is_empty() {
            self.selected_podcast = (self.selected_podcast + 1).min(self.library.len() - 1);
            self.selected_episode = 0;
            self.rebuild_selected_episodes();
        }
    }

    pub fn previous_podcast(&mut self) {
        if !self.library.is_empty() {
            self.selected_podcast = self.selected_podcast.saturating_sub(1);
            self.selected_episode = 0;
            self.rebuild_selected_episodes();
        }
    }

    pub fn next_episode(&mut self) {
        if !self.episodes.is_empty() {
            self.selected_episode = (self.selected_episode + 1).min(self.episodes.len() - 1);
        }
    }

    pub fn previous_episode(&mut self) {
        if !self.episodes.is_empty() {
            self.selected_episode = self.selected_episode.saturating_sub(1);
        }
    }

    pub fn next_queue_item(&mut self) {
        advance_index(&mut self.selected_queue, self.queue.len());
    }

    pub fn previous_queue_item(&mut self) {
        retreat_index(&mut self.selected_queue);
    }

    pub fn next_bookmark(&mut self) {
        advance_index(&mut self.selected_bookmark, self.bookmarks.len());
    }

    pub fn previous_bookmark(&mut self) {
        retreat_index(&mut self.selected_bookmark);
    }

    pub fn next_clip(&mut self) {
        advance_index(&mut self.selected_clip, self.clips.len());
    }

    pub fn previous_clip(&mut self) {
        retreat_index(&mut self.selected_clip);
    }

    pub fn next_wiki(&mut self) {
        advance_index(&mut self.selected_wiki, self.wiki_articles.len());
    }

    pub fn previous_wiki(&mut self) {
        retreat_index(&mut self.selected_wiki);
    }

    pub fn next_search_result(&mut self) {
        advance_index(&mut self.selected_search, self.search_results.len());
    }

    pub fn previous_search_result(&mut self) {
        retreat_index(&mut self.selected_search);
    }

    pub fn next_inbox_item(&mut self) {
        advance_index(&mut self.selected_inbox, self.inbox.len());
    }

    pub fn previous_inbox_item(&mut self) {
        retreat_index(&mut self.selected_inbox);
    }

    pub fn next_setting(&mut self, count: usize) {
        advance_index(&mut self.selected_setting, count);
    }

    pub fn previous_setting(&mut self) {
        retreat_index(&mut self.selected_setting);
    }

    pub fn open_episode_detail(&mut self) {
        self.mode = Mode::EpisodeDetail { scroll: 0 };
    }

    pub fn close_episode_detail(&mut self) {
        if matches!(self.mode, Mode::EpisodeDetail { .. }) {
            self.mode = Mode::Normal;
        }
    }

    pub fn episode_detail_scroll_down(&mut self) {
        if let Mode::EpisodeDetail { ref mut scroll } = self.mode {
            *scroll += 1;
        }
    }

    pub fn episode_detail_scroll_up(&mut self) {
        if let Mode::EpisodeDetail { ref mut scroll } = self.mode {
            *scroll = scroll.saturating_sub(1);
        }
    }

    pub fn episode_detail_scroll_top(&mut self) {
        if let Mode::EpisodeDetail { ref mut scroll } = self.mode {
            *scroll = 0;
        }
    }

    pub(crate) fn rebuild_selected_episodes(&mut self) {
        if let Some(podcast) = self.library.get(self.selected_podcast) {
            self.episodes = podcast.episodes.clone();
            clamp_index(&mut self.selected_episode, self.episodes.len());
        } else {
            self.episodes.clear();
            self.selected_episode = 0;
        }
    }
}

pub(crate) fn clamp_index(index: &mut usize, len: usize) {
    if len == 0 {
        *index = 0;
    } else if *index >= len {
        *index = len - 1;
    }
}

fn advance_index(index: &mut usize, len: usize) {
    if len > 0 {
        *index = (*index + 1).min(len - 1);
    }
}

fn retreat_index(index: &mut usize) {
    *index = index.saturating_sub(1);
}
