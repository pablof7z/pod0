//! Rust-owned local show/episode search for the Search tab.
//!
//! Swift renders rows and navigation, but local search semantics -- followed
//! feed-backed scope, archived-episode visibility, scoring, snippets, ranking,
//! and caps -- belong in Rust.

use std::ffi::{c_char, CStr, CString};

use podcast_core::{strip_html, TriageDecision};
use serde::{Deserialize, Serialize};

use crate::ffi::guard::ffi_guard;
use crate::ffi::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct LocalSearchRequest {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct LocalSearchResponse {
    shows: Vec<LocalShowSearchHit>,
    episodes: Vec<LocalEpisodeSearchHit>,
}

#[derive(Debug, Serialize)]
struct LocalShowSearchHit {
    podcast_id: String,
    score: i32,
}

#[derive(Debug, Serialize)]
struct LocalEpisodeSearchHit {
    episode_id: String,
    podcast_id: String,
    snippet: String,
    score: i32,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_local_search(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_local_search", std::ptr::null_mut, || {
        let raw = unsafe { CStr::from_ptr(request_json) }
            .to_string_lossy()
            .into_owned();
        let Ok(request) = serde_json::from_str::<LocalSearchRequest>(&raw) else {
            return std::ptr::null_mut();
        };
        let query = request.query.trim();
        if query.is_empty() {
            return json_response(LocalSearchResponse {
                shows: Vec::new(),
                episodes: Vec::new(),
            });
        }
        let limit = request.limit.unwrap_or(8).max(1).min(50);
        let tokens = tokenize(query);
        let handle_ref = unsafe { &*handle };
        let mut shows = Vec::new();
        let mut episodes = Vec::new();
        let Ok(store) = handle_ref.state.library.store.lock() else {
            return std::ptr::null_mut();
        };
        for (podcast, podcast_episodes) in store.all_podcasts() {
            if !store.is_subscribed(podcast.id) || podcast.feed_url.is_none() {
                continue;
            }
            let show_score = score_fields(
                vec![
                    (podcast.title.clone(), 8),
                    (podcast.author.clone(), 4),
                    (strip_html(&podcast.description), 2),
                    (podcast.categories.join(" "), 2),
                ],
                query,
                &tokens,
            );
            if show_score > 0 {
                shows.push((
                    show_score,
                    podcast.title.to_ascii_lowercase(),
                    podcast.id.0.to_string(),
                    LocalShowSearchHit {
                        podcast_id: podcast.id.0.to_string(),
                        score: show_score,
                    },
                ));
            }
            for episode in podcast_episodes {
                if episode.triage_decision.as_ref() == Some(&TriageDecision::Archived) {
                    continue;
                }
                let people = episode
                    .persons
                    .as_ref()
                    .map(|rows| {
                        rows.iter()
                            .map(|person| person.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                let sound_bites = episode
                    .sound_bites
                    .as_ref()
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|bite| bite.title.as_deref())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                let summary = strip_html(&episode.description);
                let fields = vec![
                    (episode.title.clone(), 8),
                    (podcast.title.clone(), 4),
                    (people, 3),
                    (sound_bites, 3),
                    (summary, 2),
                ];
                let episode_score = score_fields(fields.clone(), query, &tokens);
                if episode_score <= 0 {
                    continue;
                }
                episodes.push((
                    episode_score,
                    episode.pub_date.timestamp(),
                    episode.id.0.to_string(),
                    LocalEpisodeSearchHit {
                        episode_id: episode.id.0.to_string(),
                        podcast_id: podcast.id.0.to_string(),
                        snippet: best_snippet(fields.into_iter().map(|(text, _)| text).collect(), query, &tokens),
                        score: episode_score,
                    },
                ));
            }
        }
        shows.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        episodes.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        shows.truncate(limit);
        episodes.truncate(limit);
        json_response(LocalSearchResponse {
            shows: shows.into_iter().map(|(_, _, _, row)| row).collect(),
            episodes: episodes.into_iter().map(|(_, _, _, row)| row).collect(),
        })
    })
}

fn json_response(response: LocalSearchResponse) -> *mut c_char {
    match serde_json::to_string(&response) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(str::to_owned)
        .collect()
}

fn score_fields(fields: Vec<(String, i32)>, query: &str, tokens: &[String]) -> i32 {
    let needle = query.to_lowercase();
    let mut total = 0;
    for (text, weight) in fields {
        let haystack = text.to_lowercase();
        if haystack.trim().is_empty() {
            continue;
        }
        if haystack == needle {
            total += weight * 8;
        }
        if haystack.contains(&needle) {
            total += weight * 4;
        }
        for token in tokens {
            if haystack.contains(token) {
                total += weight;
            }
        }
    }
    total
}

fn best_snippet(fields: Vec<String>, query: &str, tokens: &[String]) -> String {
    let cleaned: Vec<String> = fields
        .into_iter()
        .map(|text| clean_snippet(&text))
        .filter(|text| !text.is_empty())
        .collect();
    let needle = query.to_lowercase();
    if let Some(exact) = cleaned
        .iter()
        .find(|text| text.to_ascii_lowercase().contains(&needle))
    {
        return exact.clone();
    }
    cleaned
        .into_iter()
        .max_by_key(|text| token_hits(text, tokens))
        .unwrap_or_default()
}

fn token_hits(text: &str, tokens: &[String]) -> usize {
    let lower = text.to_lowercase();
    tokens.iter().filter(|token| lower.contains(*token)).count()
}

fn clean_snippet(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
