//! Podcast action concrete handlers for owned shows, episode metadata/triage,
//! search, and app settings.
//!
//! The download-lifecycle handlers (user/auto downloads, deferred-Wi-Fi
//! dispatch, on-device model downloads, auto-download policy + evaluation,
//! deletion) live in the sibling `podcast_actions_downloads.rs` so both files
//! stay under the 500-line hard limit.
//!
//! Lock discipline (inherited from the parent module):
//! * Never hold a `PodcastStore` lock across a capability dispatch.
//! * Notifications + auto-downloads fire AFTER the store lock is released.

use std::sync::atomic::Ordering;

use podcast_feeds::http::{HttpRequest, HttpResult};

use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_create_podcast(
        &self,
        podcast_id: String,
        title: String,
        description: String,
        author: String,
        feed_url: Option<String>,
        artwork_url: Option<String>,
        language: Option<String>,
        categories: Vec<String>,
        visibility: Option<String>,
        title_is_placeholder: bool,
    ) -> serde_json::Value {
        let visibility = match visibility.as_deref() {
            Some("private") => podcast_core::NostrVisibility::Private,
            _ => podcast_core::NostrVisibility::Public,
        };
        let inserted = match self.store.lock() {
            Ok(mut s) => s.create_podcast(
                &podcast_id,
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
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !inserted {
            return serde_json::json!({
                "ok": false,
                "error": format!("invalid podcast id: {podcast_id}")
            });
        }
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true})
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_add_episode(
        &self,
        podcast_id: String,
        episode_id: String,
        title: String,
        enclosure_url: String,
        description: String,
        duration_secs: Option<f64>,
        image_url: Option<String>,
        chapters: Vec<crate::ffi::actions::podcast_module::EpisodeChapterArg>,
        transcript: Option<String>,
    ) -> serde_json::Value {
        let chapters: Vec<crate::store::owned_ext::EpisodeChapter> = chapters
            .into_iter()
            .map(|c| crate::store::owned_ext::EpisodeChapter {
                start_secs: c.start_secs,
                title: c.title,
                image_url: c.image_url,
                source_episode_id: c.source_episode_id,
            })
            .collect();
        let inserted = match self.store.lock() {
            Ok(mut s) => s.add_episode(
                &podcast_id,
                &episode_id,
                title,
                &enclosure_url,
                description,
                duration_secs,
                image_url,
                chapters,
                transcript,
            ),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !inserted {
            return serde_json::json!({
                "ok": false,
                "error": format!("could not add episode {episode_id} under podcast {podcast_id}")
            });
        }
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true, "episode_id": episode_id})
    }

    pub(super) fn handle_set_episode_triage(
        &self,
        decisions: Vec<crate::ffi::actions::podcast_module::EpisodeTriagePatch>,
    ) -> serde_json::Value {
        match self.store.lock() {
            Ok(mut s) => {
                let mut changed = false;
                for patch in decisions {
                    if s.set_episode_triage(
                        patch.episode_id,
                        &patch.decision,
                        patch.is_hero,
                        patch.rationale,
                    ) {
                        changed = true;
                    }
                }
                // Single rev bump for the whole batch — one snapshot tick.
                if changed {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    pub(super) fn handle_mark_episodes_metadata_indexed(
        &self,
        episode_ids: Vec<String>,
    ) -> serde_json::Value {
        match self.store.lock() {
            Ok(mut s) => {
                let changed = s.mark_episodes_metadata_indexed(episode_ids);
                if changed {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    pub(super) fn handle_set_episode_transcript_status(
        &self,
        episode_id: String,
        status: String,
        message: Option<String>,
        provider: Option<String>,
    ) -> serde_json::Value {
        match self.store.lock() {
            Ok(mut s) => {
                use crate::store::events::{stage, EventDetail, EventSeverity};
                // "skipped" is an event-only signal: the iOS pipeline declined
                // to transcribe (per-category opt-out, automatic AI transcription
                // off, no provider key, on-device audio missing). Record *why* in
                // the Diagnostics log WITHOUT touching `set_transcript_status`
                // (which would persist a bogus override that projects back into
                // `transcriptState`), and WITHOUT a `rev` bump — a skip changes no
                // projected state, and the sheet reads this per-episode log
                // directly (off the main-thread snapshot path).
                // `record_transcript_skip` is idempotent (see its docs) so the
                // repeatable speculative ingests don't pile duplicate rows.
                if status == "skipped" {
                    s.record_transcript_skip(&episode_id, message);
                    return serde_json::json!({"ok": true});
                }
                let changed = s.set_transcript_status(episode_id.clone(), &status, message.clone());
                if changed {
                    // Mirror the iOS-reported transcript stage into the episode
                    // pipeline log so the Diagnostics sheet shows the attempt,
                    // its provider stage, and any failure. The kernel never runs
                    // STT itself — it records what the iOS capability reports.
                    // A `Service` detail row, present only when iOS named the
                    // provider. Shared by the attempt + failure arms so the log
                    // says *which* STT service was running, not just the stage.
                    let service_detail = provider
                        .as_deref()
                        .map(|p| EventDetail::new("Service", p));
                    match status.as_str() {
                        "none" | "" => {} // status cleared — not a pipeline event
                        "failed" => {
                            let mut details = Vec::with_capacity(2);
                            if let Some(detail) = service_detail {
                                details.push(detail);
                            }
                            if let Some(m) = message {
                                details.push(EventDetail::new("Error", m));
                            }
                            s.emit_event(
                                &episode_id,
                                stage::TRANSCRIPT_FAILED,
                                EventSeverity::Failure,
                                provider
                                    .as_deref()
                                    .map(|p| format!("Transcription failed · {p}"))
                                    .unwrap_or_else(|| "Transcription failed".to_owned()),
                                details,
                            );
                        }
                        "fetching_publisher" => s.emit_event_simple(
                            &episode_id,
                            stage::TRANSCRIPT_ATTEMPT,
                            EventSeverity::Info,
                            "Fetching publisher transcript",
                        ),
                        "transcribing" => s.emit_event(
                            &episode_id,
                            stage::TRANSCRIPT_ATTEMPT,
                            EventSeverity::Info,
                            provider
                                .as_deref()
                                .map(|p| format!("Transcribing audio · {p}"))
                                .unwrap_or_else(|| "Transcribing audio".to_owned()),
                            service_detail.into_iter().collect(),
                        ),
                        "queued" => s.emit_event_simple(
                            &episode_id,
                            stage::TRANSCRIPT_ATTEMPT,
                            EventSeverity::Info,
                            "Transcription queued",
                        ),
                        other => s.emit_event_simple(
                            &episode_id,
                            stage::TRANSCRIPT_ATTEMPT,
                            EventSeverity::Info,
                            format!("Transcription: {other}"),
                        ),
                    }
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    pub(super) fn handle_search_itunes(
        &self,
        query: String,
        correlation_id: &str,
    ) -> serde_json::Value {
        let encoded = crate::itunes::url_encode(&query);
        let search_url = format!(
            "https://itunes.apple.com/search?media=podcast&entity=podcast&limit=25&term={encoded}"
        );
        let req = HttpRequest::get(search_url, [("Accept", "application/json")]);
        let http_result = match self.dispatch_http(&req, correlation_id) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({"ok": false, "error": e}),
        };
        let body = match &http_result {
            HttpResult::Ok { body, .. } => body.as_str(),
            HttpResult::Error { message } => {
                return serde_json::json!({"ok": false, "error": message})
            }
        };
        let results = crate::itunes::parse_itunes_results(body);
        // Step 9: write into DiscoveryState slot (canonical single source).
        match self.state.discovery.itunes_results.lock() {
            Ok(mut r) => {
                *r = results;
                self.state.infra.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "search_results poisoned"}),
        }
    }

    pub(super) fn handle_update_settings(
        &self,
        has_completed_onboarding: Option<bool>,
    ) -> serde_json::Value {
        let mut mutated = false;
        match self.store.lock() {
            Ok(mut s) => {
                if let Some(value) = has_completed_onboarding {
                    if s.has_completed_onboarding() != value {
                        s.set_onboarding_complete(value);
                        mutated = true;
                    }
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
        if mutated {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
        serde_json::json!({"ok": true})
    }

}
