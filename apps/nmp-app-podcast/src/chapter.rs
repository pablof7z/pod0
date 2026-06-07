use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use podcast_feeds::http::{HttpRequest, HttpResult};
use podcast_feeds::parse_chapters_json;

use crate::store::PodcastStore;

enum FetchChaptersOutcome {
    Stored,
    NoOp,
}

pub(crate) fn handle_fetch_chapters(
    store: &Arc<Mutex<PodcastStore>>,
    rev: &AtomicU64,
    episode_id: String,
    fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> serde_json::Value {
    match fetch_and_store_chapters(store, episode_id, fetch) {
        Ok(FetchChaptersOutcome::Stored) => {
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        Ok(FetchChaptersOutcome::NoOp) => serde_json::json!({"ok": true, "no_op": true}),
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    }
}

fn fetch_and_store_chapters(
    store: &Arc<Mutex<PodcastStore>>,
    episode_id: String,
    fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> Result<FetchChaptersOutcome, String> {
    let (chapters_url, already_loaded) = {
        let store = store.lock().map_err(|_| "store poisoned".to_owned())?;
        store
            .episode_chapters_state(&episode_id)
            .ok_or_else(|| format!("episode not found: {episode_id}"))?
    };
    let Some(url) = chapters_url else { return Ok(FetchChaptersOutcome::NoOp); };
    if already_loaded {
        return Ok(FetchChaptersOutcome::NoOp);
    }

    let req = HttpRequest::get(url.to_string(), [("Accept", "application/json")]);
    let body = match fetch(&req)? {
        HttpResult::Ok { body, .. } => body,
        HttpResult::Error { message } => return Err(message),
    };
    let chapters: Vec<podcast_core::Chapter> = parse_chapters_json(&body)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|chapter| chapter.include_in_toc)
        .collect();

    let chapter_count = chapters.len();
    let mut store = store.lock().map_err(|_| "store poisoned".to_owned())?;
    if store.set_episode_chapters(&episode_id, chapters) {
        if chapter_count > 0 {
            // Chapter identification landed (RSS / Podcasting 2.0 source).
            store.emit_event(
                &episode_id,
                crate::store::events::stage::CHAPTERS_READY,
                crate::store::events::EventSeverity::Success,
                "Chapters identified",
                vec![
                    crate::store::events::EventDetail::new("Count", chapter_count.to_string()),
                    crate::store::events::EventDetail::new("Source", "RSS".to_owned()),
                ],
            );
        }
        Ok(FetchChaptersOutcome::Stored)
    } else {
        Err("episode disappeared mid-fetch".to_owned())
    }
}
