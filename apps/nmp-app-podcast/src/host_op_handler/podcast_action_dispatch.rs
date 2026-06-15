//! `PodcastAction` dispatch.
//!
//! This module owns the single match over podcast actions and delegates concrete
//! work to focused sibling impl modules.

use crate::chapter::handle_fetch_chapters;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::host_op_handler::PodcastHostOpHandler;

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
            PodcastAction::FetchTranscript { episode_id } => self
                .state
                .transcripts
                .handle_fetch(episode_id, |req| self.dispatch_http(req, correlation_id)),
            PodcastAction::FetchChapters { episode_id } => {
                // Step 15: store/rev sourced from state.library/state.infra.
                handle_fetch_chapters(
                    &self.state.library.store,
                    &self.state.infra.rev,
                    episode_id,
                    |req| self.dispatch_http(req, correlation_id),
                )
            }
            PodcastAction::UpdateSettings {
                has_completed_onboarding,
            } => self.handle_update_settings(has_completed_onboarding),
            PodcastAction::FetchComments { episode_id } => {
                // Step 8: use CommentsState slots from the shared state.
                crate::comments_handler::handle_fetch_comments(
                    self.app,
                    &self.state.comments.store,
                    &self.state.comments.viewed_episode_id.share(),
                    &self.state.infra.rev,
                    self.state.comments.infra.signal.as_ref(),
                    &episode_id,
                )
            }
            PodcastAction::PostComment {
                episode_id,
                content,
            } => crate::comments_handler::handle_post_comment(
                self.app,
                &self.state.comments.store,
                &self.state.comments.identity,
                &self.state.comments.cache.share(),
                &self.state.infra.rev,
                &episode_id,
                &content,
            ),
            PodcastAction::SetAutoDownload {
                podcast_id,
                mode,
                count,
                enabled,
                wifi_only,
            } => self.handle_set_auto_download(podcast_id, mode, count, enabled, wifi_only, correlation_id),
            PodcastAction::DispatchDeferredWifiDownloads => {
                self.handle_dispatch_deferred_wifi_downloads(correlation_id)
            }
            PodcastAction::AutoDownloadEvaluate => {
                self.handle_evaluate_auto_downloads(correlation_id)
            }
            PodcastAction::FetchContacts => {
                // Reactive path: the FollowListObserver registered in register.rs
                // populates social_slot automatically on every kind:3 push frame.
                // This call is a lightweight refresh trigger — no relay pull.
                // Pass the Domain::Social-scoped Infra so handle_fetch_contacts
                // bumps domain_revs.social (driving the podcast.social sidecar
                // re-emit) in addition to the global rev.
                crate::social_handler::handle_fetch_contacts(
                    self.state.social.social_slot.share(),
                    Some(&self.state.social.infra),
                    self.state.infra.rev.clone(),
                    self.state.infra.signal.as_ref(),
                )
            }
            PodcastAction::PublishAgentNote {
                recipient_pubkey_hex,
                content,
                root_event_id,
                inbound_event_id,
                root_a_tags,
            } => crate::agent_note_handler::handle_publish_agent_note(
                self.app,
                &self.state.library.identity,
                &recipient_pubkey_hex,
                &content,
                root_event_id.as_deref(),
                inbound_event_id.as_deref(),
                &root_a_tags,
            ),
            PodcastAction::FetchAgentNotes => {
                crate::agent_note_handler::handle_fetch_agent_notes(
                    self.app,
                    &self.state.library.identity,
                )
            }
            PodcastAction::StarEpisode {
                episode_id,
                starred,
            } => match self.state.library.store.lock() {
                Ok(mut s) => match s.set_episode_starred(&episode_id, starred) {
                    Some(new_value) => {
                        self.bump_domain(crate::state::Domain::Library);
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
                provider,
            } => self.handle_set_episode_transcript_status(episode_id, status, message, provider),
            PodcastAction::SummarizeEpisode { episode_id } => {
                // Step 15+16: store/rev/runtime/signal sourced from state.*.
                if let Some(signal) = self.state.infra.signal.clone() {
                    crate::episode_summary::handle_summarize_episode_with_signal(
                        &self.state.library.store,
                        &self.state.infra.rev,
                        &self.state.infra.runtime,
                        episode_id,
                        signal,
                    )
                } else {
                    crate::episode_summary::handle_summarize_episode(
                        &self.state.library.store,
                        &self.state.infra.rev,
                        &self.state.infra.runtime,
                        episode_id,
                    )
                }
            }
            // Step 16: feedback is now in state.feedback.
            PodcastAction::FetchFeedback => self.state.feedback.fetch(self.app).as_json(),
            PodcastAction::PublishFeedback {
                category,
                content,
                parent_event_id,
                reply_to_pubkey,
            } => self
                .state.feedback
                .publish(
                    self.app,
                    &category,
                    &content,
                    parent_event_id.as_deref(),
                    reply_to_pubkey.as_deref(),
                )
                .as_json(),
            PodcastAction::SubscribeNostr {
                author_pubkey_hex,
                show_title,
            } => crate::nostr_episodes::handle_subscribe_nostr(
                self.app,
                &self.state.library.store,
                &self.state.infra.rev,
                self.state.infra.signal.as_ref(),
                &author_pubkey_hex,
                show_title.as_deref(),
            ),
            // DiscoverNostr is handled in PodcastActionModule::execute via
            // EnsureInterest/DropInterestOwner before reaching the host-op
            // handler; it never arrives here.
            PodcastAction::DiscoverNostr { .. } => {
                serde_json::json!({"ok": false, "error": "discover_nostr must be handled by execute()"})
            }
            PodcastAction::SetPodcastUserCategories {
                podcast_id,
                categories,
            } => self.handle_set_podcast_user_categories(podcast_id, categories),
            PodcastAction::SetPodcastTranscriptionEnabled {
                podcast_id,
                enabled,
            } => self.handle_set_podcast_transcription_enabled(podcast_id, enabled),
        }
    }
}
