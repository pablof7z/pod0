//! Rust-owned agent inventory queries.
//!
//! Swift still owns the type-erased agent protocol surface, but the kernel
//! owns inventory scope, counts, filters, ordering, and caps.

use std::ffi::{c_char, CStr, CString};

use podcast_core::{Episode, Podcast, PodcastId, TriageDecision};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ffi::guard::ffi_guard;
use crate::ffi::handle::PodcastHandle;

const UNKNOWN_PODCAST_ID: &str = "00000000-eeee-eeee-eeee-000000000000";

#[derive(Debug, Deserialize)]
struct AgentInventoryRequest {
    op: String,
    #[serde(default)]
    podcast_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct AgentInventoryResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subscriptions: Vec<SubscriptionRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    podcasts: Vec<PodcastRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    episodes: Vec<EpisodeRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    found: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SubscriptionRow {
    podcast_id: String,
    title: String,
    author: Option<String>,
    total_episodes: usize,
    unplayed_episodes: usize,
    last_published_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct PodcastRow {
    podcast_id: String,
    title: String,
    author: Option<String>,
    subscribed: bool,
    total_episodes: usize,
    unplayed_episodes: usize,
    last_published_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct EpisodeRow {
    episode_id: String,
    podcast_id: String,
    title: String,
    podcast_title: String,
    published_at: Option<i64>,
    duration_seconds: Option<i64>,
    played: bool,
    playback_position_seconds: f64,
    is_in_progress: bool,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_inventory(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_agent_inventory", std::ptr::null_mut, || {
        let raw = unsafe { CStr::from_ptr(request_json) }
            .to_string_lossy()
            .into_owned();
        let request = match serde_json::from_str::<AgentInventoryRequest>(&raw) {
            Ok(request) => request,
            Err(e) => return response_json(AgentInventoryResponse::error(e.to_string())),
        };
        let limit = request.limit.unwrap_or(30).max(1).min(200);
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => match request.op.as_str() {
                "list_subscriptions" => {
                    let mut rows: Vec<SubscriptionRow> = store
                        .subscribed_podcasts()
                        .into_iter()
                        .filter(|(podcast, _)| !is_unknown_podcast(podcast))
                        .map(subscription_row)
                        .collect();
                    sort_subscriptions(&mut rows);
                    rows.truncate(limit);
                    AgentInventoryResponse::subscriptions(rows)
                }
                "list_podcasts" => {
                    let mut rows: Vec<PodcastRow> = store
                        .all_podcasts()
                        .into_iter()
                        .filter(|(podcast, _)| !is_unknown_podcast(podcast))
                        .map(|(podcast, episodes)| {
                            podcast_row(podcast, episodes, store.is_subscribed(podcast.id))
                        })
                        .collect();
                    sort_podcasts(&mut rows);
                    rows.truncate(limit);
                    AgentInventoryResponse::podcasts(rows)
                }
                "list_episodes" => {
                    let Some(id) = request.podcast_id.as_deref() else {
                        return response_json(AgentInventoryResponse::error(
                            "missing podcast_id".to_string(),
                        ));
                    };
                    let Ok(uuid) = Uuid::parse_str(id) else {
                        return response_json(AgentInventoryResponse::error(
                            format!("invalid podcast_id: {id}"),
                        ));
                    };
                    let podcast_id = PodcastId::new(uuid);
                    let Some(podcast) = store.podcast(podcast_id) else {
                        return response_json(AgentInventoryResponse {
                            subscriptions: Vec::new(),
                            podcasts: Vec::new(),
                            episodes: Vec::new(),
                            found: Some(false),
                            error: None,
                        });
                    };
                    let mut episodes: Vec<&Episode> =
                        store.episodes_for(podcast_id).iter().collect();
                    sort_episodes_newest_first(&mut episodes);
                    let rows = episodes
                        .into_iter()
                        .take(limit)
                        .map(|episode| episode_row(episode, &podcast.title))
                        .collect();
                    AgentInventoryResponse {
                        subscriptions: Vec::new(),
                        podcasts: Vec::new(),
                        episodes: rows,
                        found: Some(true),
                        error: None,
                    }
                }
                "list_in_progress" => {
                    let mut rows = all_episode_rows(&store)
                        .into_iter()
                        .filter(|(_, episode)| {
                            !episode.played && !is_archived(episode) && episode.position_secs > 0.0
                        })
                        .collect::<Vec<_>>();
                    sort_episode_pairs_newest_first(&mut rows);
                    AgentInventoryResponse::episodes(
                        rows.into_iter()
                            .take(limit)
                            .map(|(podcast, episode)| episode_row(episode, &podcast.title))
                            .collect(),
                    )
                }
                "list_recent_unplayed" => {
                    let mut rows = all_episode_rows(&store)
                        .into_iter()
                        .filter(|(_, episode)| {
                            !episode.played && !is_archived(episode) && episode.position_secs <= 0.0
                        })
                        .collect::<Vec<_>>();
                    sort_episode_pairs_newest_first(&mut rows);
                    AgentInventoryResponse::episodes(
                        rows.into_iter()
                            .take(limit)
                            .map(|(podcast, episode)| episode_row(episode, &podcast.title))
                            .collect(),
                    )
                }
                other => AgentInventoryResponse::error(format!("unknown inventory op: {other}")),
            },
            Err(_) => AgentInventoryResponse::error("store poisoned".to_string()),
        };
        response_json(response)
    })
}

impl AgentInventoryResponse {
    fn subscriptions(subscriptions: Vec<SubscriptionRow>) -> Self {
        Self {
            subscriptions,
            podcasts: Vec::new(),
            episodes: Vec::new(),
            found: None,
            error: None,
        }
    }

    fn podcasts(podcasts: Vec<PodcastRow>) -> Self {
        Self {
            subscriptions: Vec::new(),
            podcasts,
            episodes: Vec::new(),
            found: None,
            error: None,
        }
    }

    fn episodes(episodes: Vec<EpisodeRow>) -> Self {
        Self {
            subscriptions: Vec::new(),
            podcasts: Vec::new(),
            episodes,
            found: None,
            error: None,
        }
    }

    fn error(error: String) -> Self {
        Self {
            subscriptions: Vec::new(),
            podcasts: Vec::new(),
            episodes: Vec::new(),
            found: None,
            error: Some(error),
        }
    }
}

fn response_json(response: AgentInventoryResponse) -> *mut c_char {
    match serde_json::to_string(&response) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

fn all_episode_rows<'a>(
    store: &'a crate::store::PodcastStore,
) -> Vec<(&'a Podcast, &'a Episode)> {
    store
        .all_podcasts()
        .into_iter()
        .filter(|(podcast, _)| !is_unknown_podcast(podcast))
        .flat_map(|(podcast, episodes)| episodes.iter().map(move |episode| (podcast, episode)))
        .collect()
}

fn subscription_row((podcast, episodes): (&Podcast, &[Episode])) -> SubscriptionRow {
    SubscriptionRow {
        podcast_id: podcast.id.0.to_string(),
        title: podcast.title.clone(),
        author: non_empty(podcast.author.clone()),
        total_episodes: episodes.len(),
        unplayed_episodes: unplayed_count(episodes),
        last_published_at: last_published_at(episodes),
    }
}

fn podcast_row(podcast: &Podcast, episodes: &[Episode], subscribed: bool) -> PodcastRow {
    PodcastRow {
        podcast_id: podcast.id.0.to_string(),
        title: podcast.title.clone(),
        author: non_empty(podcast.author.clone()),
        subscribed,
        total_episodes: episodes.len(),
        unplayed_episodes: unplayed_count(episodes),
        last_published_at: last_published_at(episodes),
    }
}

fn episode_row(episode: &Episode, podcast_title: &str) -> EpisodeRow {
    EpisodeRow {
        episode_id: episode.id.0.to_string(),
        podcast_id: episode.podcast_id.0.to_string(),
        title: episode.title.clone(),
        podcast_title: podcast_title.to_string(),
        published_at: Some(episode.pub_date.timestamp()),
        duration_seconds: episode.duration_secs.map(|v| v.round() as i64),
        played: episode.played,
        playback_position_seconds: episode.position_secs,
        is_in_progress: !episode.played && episode.position_secs > 0.0,
    }
}

fn unplayed_count(episodes: &[Episode]) -> usize {
    episodes
        .iter()
        .filter(|episode| !episode.played && !is_archived(episode))
        .count()
}

fn last_published_at(episodes: &[Episode]) -> Option<i64> {
    episodes
        .iter()
        .map(|episode| episode.pub_date.timestamp())
        .max()
}

fn is_archived(episode: &Episode) -> bool {
    episode.triage_decision == Some(TriageDecision::Archived)
}

fn is_unknown_podcast(podcast: &Podcast) -> bool {
    podcast.id.0.to_string().eq_ignore_ascii_case(UNKNOWN_PODCAST_ID)
}

fn non_empty(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn sort_subscriptions(rows: &mut [SubscriptionRow]) {
    rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
}

fn sort_podcasts(rows: &mut [PodcastRow]) {
    rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
}

fn sort_episodes_newest_first(rows: &mut [&Episode]) {
    rows.sort_by(|a, b| {
        b.pub_date
            .cmp(&a.pub_date)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
}

fn sort_episode_pairs_newest_first(rows: &mut [(&Podcast, &Episode)]) {
    rows.sort_by(|a, b| {
        b.1.pub_date
            .cmp(&a.1.pub_date)
            .then_with(|| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase()))
    });
}
