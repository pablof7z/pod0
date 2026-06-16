use std::collections::HashSet;

use nmp_app_podcast::ffi::PodcastUpdate;

use crate::app::{clamp_index, AppState, NowPlaying};
use crate::rows::{DownloadRow, EpisodeRow, InboxRow, PodcastRow, SearchResult};

impl AppState {
    pub fn apply_podcast_update(&mut self, update: PodcastUpdate) {
        self.update_count += 1;

        self.library = update.library.into_iter().map(PodcastRow::from).collect();
        clamp_index(&mut self.selected_podcast, self.library.len());
        self.rebuild_selected_episodes();
        self.rebuild_bookmarks();

        self.now_playing = update.now_playing.map(|np| {
            let episode_id = np.episode_id.unwrap_or_default();
            let (podcast_title, episode_title) = self.find_episode_titles(&episode_id);
            NowPlaying {
                episode_id,
                podcast_title,
                episode_title,
                position_secs: np.position_secs,
                duration_secs: np.duration_secs,
                is_playing: np.is_playing,
                speed: np.speed,
                volume: np.volume,
            }
        });

        self.queue = update.queue.into_iter().map(EpisodeRow::from).collect();
        clamp_index(&mut self.selected_queue, self.queue.len());

        self.search_results = update
            .search_results
            .into_iter()
            .map(SearchResult::from)
            .collect();
        clamp_index(&mut self.selected_search, self.search_results.len());

        self.inbox = update.inbox.into_iter().map(InboxRow::from).collect();
        clamp_index(&mut self.selected_inbox, self.inbox.len());
        self.inbox_triage_in_progress = update.inbox_triage_in_progress;

        self.apply_downloads(update.downloads);

        self.settings = update.settings;
        self.clips = update.clips;
        clamp_index(&mut self.selected_clip, self.clips.len());


        if let Some(agent) = update.agent {
            self.agent_messages = agent.messages;
            self.agent_is_busy = agent.is_busy;
        } else {
            self.agent_messages.clear();
            self.agent_is_busy = false;
        }
        self.agent_picks = update.picks;
        self.agent_tasks = update.agent_tasks;
        self.nostr_conversations = update.nostr_conversations;
        self.memory_facts = update.memory_facts;
        clamp_index(&mut self.selected_agent_pick, self.agent_picks.len());
        clamp_index(&mut self.selected_agent_task, self.agent_tasks.len());
        clamp_index(&mut self.selected_agent_note, self.nostr_conversations.len());
        clamp_index(&mut self.selected_memory_fact, self.memory_facts.len());
        self.comments = update.comments;
        self.categories = update.categories;
        self.configured_relays = update.configured_relays;
        clamp_index(&mut self.selected_relay, self.configured_relays.len());
        self.active_account = update.active_account;
        if let Some(social) = update.social {
            self.social_following_count = social.following_count;
            self.social_contacts = social.following;
        } else {
            self.social_following_count = 0;
            self.social_contacts.clear();
        }

        if let Some(toast) = update.toast {
            self.push_toast(&toast);
        }

        self.status = format!(
            "update #{} ({} podcasts, {} queued, {} clips)",
            self.update_count,
            self.library.len(),
            self.queue.len(),
            self.clips.len()
        );
    }

    fn rebuild_bookmarks(&mut self) {
        self.bookmarks = self
            .library
            .iter()
            .flat_map(|podcast| podcast.episodes.iter())
            .filter(|episode| episode.starred)
            .cloned()
            .collect();
        clamp_index(&mut self.selected_bookmark, self.bookmarks.len());
    }

    fn find_episode_titles(&self, episode_id: &str) -> (String, String) {
        for podcast in &self.library {
            for episode in &podcast.episodes {
                if episode.id == episode_id {
                    return (podcast.title.clone(), episode.title.clone());
                }
            }
        }
        (String::new(), String::new())
    }

    fn apply_downloads(&mut self, downloads: Option<nmp_app_podcast::ffi::DownloadQueueSnapshot>) {
        let Some(downloads) = downloads else {
            return;
        };

        let previous_ids: HashSet<String> = self
            .downloads
            .iter()
            .map(|download| download.episode_id.clone())
            .collect();
        self.downloads = downloads
            .active
            .into_iter()
            .map(DownloadRow::from)
            .collect();
        clamp_index(&mut self.selected_download, self.downloads.len());

        for previous_id in &previous_ids {
            if !self
                .downloads
                .iter()
                .any(|download| &download.episode_id == previous_id)
            {
                self.push_toast(&format!("download complete: {previous_id}"));
            }
        }
    }
}
