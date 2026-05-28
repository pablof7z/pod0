use serde_json::Value;

use crate::bridge::NmpEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Library,
    Episodes,
    Player,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Library,
    Queue,
    Search,
    Settings,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Library => "library",
            Tab::Queue => "queue",
            Tab::Search => "search",
            Tab::Settings => "settings",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Library => Tab::Queue,
            Tab::Queue => Tab::Search,
            Tab::Search => Tab::Settings,
            Tab::Settings => Tab::Library,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Tab::Library => Tab::Settings,
            Tab::Queue => Tab::Library,
            Tab::Search => Tab::Queue,
            Tab::Settings => Tab::Search,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    SearchInput,
    SubscribeInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PodcastRow {
    pub id: String,
    pub title: String,
    pub unplayed_count: usize,
    pub episodes: Vec<EpisodeRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeRow {
    pub id: String,
    pub title: String,
    pub duration_secs: Option<f64>,
    pub playback_position_secs: Option<f64>,
    pub played: bool,
    pub starred: bool,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub message: String,
    pub ttl_ticks: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub artwork_url: Option<String>,
    pub feed_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub focused: Pane,
    pub tab: Tab,
    pub mode: Mode,
    pub show_help: bool,
    pub update_count: u64,
    pub library: Vec<PodcastRow>,
    pub episodes: Vec<EpisodeRow>,
    pub selected_podcast: usize,
    pub selected_episode: usize,
    pub now_playing: Option<NowPlaying>,
    pub queue: Vec<EpisodeRow>,
    pub search_results: Vec<SearchResult>,
    pub selected_search: usize,
    pub search_input: String,
    pub subscribe_input: String,
    pub status: String,
    pub toasts: Vec<Toast>,
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
            library: Vec::new(),
            episodes: Vec::new(),
            selected_podcast: 0,
            selected_episode: 0,
            now_playing: None,
            queue: Vec::new(),
            search_results: Vec::new(),
            selected_search: 0,
            search_input: String::new(),
            subscribe_input: String::new(),
            status: "starting kernel".to_string(),
            toasts: Vec::new(),
            playback_error: None,
        }
    }
}

impl AppState {
    pub fn apply_nmp_event(&mut self, event: NmpEvent) {
        self.update_count += 1;
        match serde_json::from_str::<Value>(&event.payload) {
            Ok(value) => self.apply_snapshot(value),
            Err(e) => {
                self.status = format!("snapshot parse error: {e}");
            }
        }
    }

    fn apply_snapshot(&mut self, value: Value) {
        // Library
        if let Some(library) = value.get("library").and_then(Value::as_array) {
            self.library = library.iter().filter_map(parse_podcast_row).collect();
            if self.selected_podcast >= self.library.len() {
                self.selected_podcast = self.library.len().saturating_sub(1);
            }
        }

        // Episodes for selected podcast
        if let Some(podcasts) = value.get("library").and_then(Value::as_array) {
            if let Some(podcast) = podcasts.get(self.selected_podcast) {
                if let Some(eps) = podcast.get("episodes").and_then(Value::as_array) {
                    self.episodes = eps.iter().filter_map(parse_episode_row).collect();
                    if self.selected_episode >= self.episodes.len() {
                        self.selected_episode = self.episodes.len().saturating_sub(1);
                    }
                } else {
                    self.episodes.clear();
                }
            }
        }

        // Now playing
        if let Some(np) = value.get("now_playing") {
            if np.is_null() || np.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                if let Some(ref mut np) = self.now_playing {
                    np.is_playing = false;
                }
            } else {
                let id = np
                    .get("episode_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let podcast_title = np
                    .get("podcast_title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let episode_title = np
                    .get("episode_title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let position_secs = np
                    .get("position_secs")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let duration_secs = np
                    .get("duration_secs")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let is_playing = np
                    .get("is_playing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let speed = np.get("speed").and_then(Value::as_f64).unwrap_or(1.0) as f32;
                let volume = np.get("volume").and_then(Value::as_f64).unwrap_or(1.0) as f32;

                self.now_playing = Some(NowPlaying {
                    episode_id: id,
                    podcast_title,
                    episode_title,
                    position_secs,
                    duration_secs,
                    is_playing,
                    speed,
                    volume,
                });
            }
        }

        // Queue
        if let Some(queue) = value.get("queue").and_then(Value::as_array) {
            self.queue = queue.iter().filter_map(parse_episode_row).collect();
        }

        // Search results
        if let Some(results) = value.get("search_results").and_then(Value::as_array) {
            self.search_results = results.iter().filter_map(parse_search_result).collect();
            if self.selected_search >= self.search_results.len() {
                self.selected_search = self.search_results.len().saturating_sub(1);
            }
        }

        // Toast
        if let Some(toast) = value.get("toast").and_then(Value::as_str) {
            self.push_toast(toast);
        }

        self.status =
            format!("update #{} ({} podcasts)", self.update_count, self.library.len());
    }

    pub fn selected_podcast_id(&self) -> Option<String> {
        self.library.get(self.selected_podcast).map(|p| p.id.clone())
    }

    pub fn selected_episode_id(&self) -> Option<String> {
        self.episodes.get(self.selected_episode).map(|e| e.id.clone())
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
        }
    }

    pub fn previous_podcast(&mut self) {
        if !self.library.is_empty() {
            self.selected_podcast = self.selected_podcast.saturating_sub(1);
            self.selected_episode = 0;
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
        // queue selection is not yet implemented
    }

    pub fn previous_queue_item(&mut self) {
        // queue selection is not yet implemented
    }

    pub fn selected_search_result_id(&self) -> Option<String> {
        self.search_results.get(self.selected_search).map(|r| r.id.clone())
    }

    pub fn selected_search_feed_url(&self) -> Option<String> {
        self.search_results.get(self.selected_search).and_then(|r| r.feed_url.clone())
    }

    pub fn next_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.selected_search = (self.selected_search + 1).min(self.search_results.len() - 1);
        }
    }

    pub fn previous_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.selected_search = self.selected_search.saturating_sub(1);
        }
    }
}

fn parse_podcast_row(value: &Value) -> Option<PodcastRow> {
    let id = value.get("id")?.as_str()?.to_string();
    let title = value.get("title")?.as_str()?.to_string();
    let unplayed_count = value.get("unplayed_count")?.as_u64()? as usize;
    let episodes = value
        .get("episodes")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_episode_row).collect())
        .unwrap_or_default();
    Some(PodcastRow {
        id,
        title,
        unplayed_count,
        episodes,
    })
}

fn parse_episode_row(value: &Value) -> Option<EpisodeRow> {
    let id = value.get("id")?.as_str()?.to_string();
    let title = value.get("title")?.as_str()?.to_string();
    let duration_secs = value.get("duration_secs").and_then(Value::as_f64);
    let playback_position_secs = value.get("playback_position_secs").and_then(Value::as_f64);
    let played = value.get("played").and_then(Value::as_bool).unwrap_or(false);
    let starred = value.get("starred").and_then(Value::as_bool).unwrap_or(false);
    Some(EpisodeRow {
        id,
        title,
        duration_secs,
        playback_position_secs,
        played,
        starred,
    })
}

fn parse_search_result(value: &Value) -> Option<SearchResult> {
    let id = value.get("id")?.as_str()?.to_string();
    let title = value.get("title")?.as_str()?.to_string();
    let author = value.get("author").and_then(Value::as_str).map(String::from);
    let artwork_url = value.get("artwork_url").and_then(Value::as_str).map(String::from);
    let feed_url = value.get("feed_url").and_then(Value::as_str).map(String::from);
    Some(SearchResult {
        id,
        title,
        author,
        artwork_url,
        feed_url,
    })
}
