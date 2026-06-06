//! `PodcastAction` dispatch.
//!
//! This module owns the single match over podcast actions and delegates concrete
//! work to focused sibling impl modules.

use std::sync::atomic::Ordering;

use crate::chapter::handle_fetch_chapters;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::transcript::handle_fetch_transcript;

impl PodcastHostOpHandler {
    pub(super) fn handle_podcast_action(
        &self,
        action: PodcastAction,
        correlation_id: &str,
    ) -> serde_json::Value {
        match action {
            PodcastAction::Subscribe { feed_url } => {
                self.handle_subscribe(feed_url, correlation_id)
            }
            PodcastAction::EnsurePodcast { feed_url } => {
                self.handle_ensure_podcast(feed_url, correlation_id)
            }
            PodcastAction::CreatePodcast {
                podcast_id,
                title,
                description,
                author,
                feed_url,
                artwork_url,
                language,
                categories,
                visibility,
                title_is_placeholder,
            } => self.handle_create_podcast(
                podcast_id,
                title,
                description,
                author,
                feed_url,
                artwork_url,
                language,
                categories,
                visibility,
                title_is_placeholder,
            ),
            PodcastAction::AddEpisode {
                podcast_id,
                episode_id,
                title,
                enclosure_url,
                description,
                duration_secs,
                image_url,
                chapters,
                transcript,
            } => self.handle_add_episode(
                podcast_id,
                episode_id,
                title,
                enclosure_url,
                description,
                duration_secs,
                image_url,
                chapters,
                transcript,
            ),
            PodcastAction::Unsubscribe { podcast_id } => self.handle_unsubscribe(podcast_id),
            PodcastAction::Refresh { podcast_id } => {
                self.handle_refresh(podcast_id, correlation_id)
            }
            PodcastAction::RefreshAll => self.handle_refresh_all(correlation_id),
            PodcastAction::SearchItunes { query } => {
                self.handle_search_itunes(query, correlation_id)
            }
            PodcastAction::ImportOpml { content } => {
                self.handle_import_opml(content, correlation_id)
            }
            PodcastAction::Download { episode_id, url } => {
                self.handle_download(episode_id, url, correlation_id)
            }
            PodcastAction::DeleteDownload { episode_id } => self.handle_delete_download(episode_id),
            PodcastAction::DownloadLocalModel { model_id, url } => {
                self.handle_download_local_model(model_id, url, correlation_id)
            }
            PodcastAction::FetchTranscript { episode_id } => handle_fetch_transcript(
                &self.store,
                &self.transcripts,
                &self.rev,
                episode_id,
                |req| self.dispatch_http(req, correlation_id),
            ),
            PodcastAction::FetchChapters { episode_id } => {
                handle_fetch_chapters(&self.store, &self.rev, episode_id, |req| {
                    self.dispatch_http(req, correlation_id)
                })
            }
            PodcastAction::UpdateSettings {
                has_completed_onboarding,
            } => self.handle_update_settings(has_completed_onboarding),
            PodcastAction::FetchComments { episode_id } => {
                crate::comments_handler::handle_fetch_comments(
                    self.app,
                    &self.store,
                    &self.viewed_comments_episode_id,
                    &episode_id,
                )
            }
            PodcastAction::PostComment {
                episode_id,
                content,
            } => crate::comments_handler::handle_post_comment(
                self.app,
                &self.store,
                &self.identity,
                &self.comments_cache,
                &self.rev,
                &episode_id,
                &content,
            ),
            PodcastAction::SetAutoDownload {
                podcast_id,
                enabled,
                wifi_only,
            } => self.handle_set_auto_download(podcast_id, enabled, wifi_only),
            PodcastAction::DispatchDeferredWifiDownloads => {
                self.handle_dispatch_deferred_wifi_downloads(correlation_id)
            }
            PodcastAction::FetchContacts => crate::social_handler::handle_fetch_contacts(self),
            PodcastAction::PublishAgentNote {
                recipient_pubkey_hex,
                content,
                root_event_id,
                inbound_event_id,
                root_a_tags,
            } => crate::agent_note_handler::handle_publish_agent_note(
                self.app,
                &self.identity,
                &recipient_pubkey_hex,
                &content,
                root_event_id.as_deref(),
                inbound_event_id.as_deref(),
                &root_a_tags,
            ),
            PodcastAction::FetchAgentNotes => {
                crate::agent_note_handler::handle_fetch_agent_notes(self.app, &self.identity)
            }
            PodcastAction::StarEpisode {
                episode_id,
                starred,
            } => match self.store.lock() {
                Ok(mut s) => match s.set_episode_starred(&episode_id, starred) {
                    Some(new_value) => {
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        serde_json::json!({"ok": true, "starred": new_value})
                    }
                    None => {
                        serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")})
                    }
                },
                Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
            },
            PodcastAction::SetEpisodeTriage { decisions } => {
                self.handle_set_episode_triage(decisions)
            }
            PodcastAction::MarkEpisodesMetadataIndexed { episode_ids } => {
                self.handle_mark_episodes_metadata_indexed(episode_ids)
            }
            PodcastAction::SetEpisodeTranscriptStatus {
                episode_id,
                status,
                message,
            } => self.handle_set_episode_transcript_status(episode_id, status, message),
            PodcastAction::SummarizeEpisode { episode_id } => {
                crate::episode_summary::handle_summarize_episode(
                    &self.store,
                    &self.rev,
                    &self.runtime,
                    episode_id,
                )
            }
            PodcastAction::FetchFeedback => {
                crate::feedback_handler::handle_fetch_feedback(self.app)
            }
            PodcastAction::PublishFeedback {
                category,
                content,
                parent_event_id,
                reply_to_pubkey,
            } => crate::feedback_handler::handle_publish_feedback(
                self.app,
                &category,
                &content,
                parent_event_id.as_deref(),
                reply_to_pubkey.as_deref(),
            ),
            // DiscoverNostr is handled in PodcastActionModule::execute via
            // EnsureInterest/DropInterestOwner before reaching the host-op
            // handler; it never arrives here.
            PodcastAction::DiscoverNostr { .. } => {
                serde_json::json!({"ok": false, "error": "discover_nostr must be handled by execute()"})
            }
        }
    }
}
