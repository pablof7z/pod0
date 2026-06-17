//! Rust-owned cross-episode threading projection.
//!
//! Swift used to persist threading topics/mentions and seed a DEBUG mock row.
//! This projection derives bounded thread rows from kernel library,
//! transcript, and categorization facts so native shells only render.

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};

use podcast_core::TriageDecision;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ffi::actions::categorization_module::categorize_text;
use crate::ffi::guard::ffi_guard;
use crate::ffi::handle::PodcastHandle;

const MIN_EPISODES_PER_THREAD: usize = 3;
const MAX_TOPICS: usize = 12;
const MAX_MENTIONS_PER_TOPIC: usize = 60;

#[derive(Debug, Deserialize)]
struct ActiveTopicsRequest {
    #[serde(default)]
    podcast_ids: Vec<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ActiveTopicsProjection {
    active_topics: Vec<ActiveThreadingTopicRow>,
}

#[derive(Debug, Serialize)]
struct ActiveThreadingTopicRow {
    topic_id: String,
    unplayed_episode_count: usize,
    mention_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ThreadingProjection {
    topics: Vec<ThreadingTopicRow>,
    mentions: Vec<ThreadingMentionRow>,
}

#[derive(Debug, Serialize)]
struct ThreadingTopicRow {
    id: String,
    slug: String,
    display_name: String,
    definition: Option<String>,
    episode_mention_count: usize,
    contradiction_count: usize,
    last_mentioned_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ThreadingMentionRow {
    id: String,
    topic_id: String,
    episode_id: String,
    start_ms: i64,
    end_ms: i64,
    snippet: String,
    confidence: f64,
    is_contradictory: bool,
}

#[derive(Clone)]
struct EpisodeThreadInput {
    podcast_id: String,
    episode_id: String,
    title: String,
    description: String,
    transcript: Option<String>,
    timed_entries: Vec<podcast_transcripts::TranscriptEntry>,
    published_at: i64,
    played: bool,
    triage_archived: bool,
}

#[derive(Clone)]
struct CandidateMention {
    episode_id: String,
    published_at: i64,
    start_ms: i64,
    end_ms: i64,
    snippet: String,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_threading_projection(
    handle: *mut PodcastHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_threading_projection",
        std::ptr::null_mut,
        || {
            let handle_ref = unsafe { &*handle };
            let categories = handle_ref.state.categories.categories_snapshot();
            let projection = match handle_ref.state.library.store.lock() {
                Ok(store) => {
                    let inputs = collect_thread_inputs(&store);
                    build_projection(inputs, &categories)
                }
                Err(_) => return std::ptr::null_mut(),
            };

            match serde_json::to_string(&projection) {
                Ok(json) => match CString::new(json) {
                    Ok(c) => c.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                },
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_threading_active_topics(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_threading_active_topics",
        std::ptr::null_mut,
        || {
            let request = if request_json.is_null() {
                ActiveTopicsRequest {
                    podcast_ids: Vec::new(),
                    limit: Some(1),
                }
            } else {
                let raw = unsafe { CStr::from_ptr(request_json) }
                    .to_string_lossy()
                    .into_owned();
                serde_json::from_str::<ActiveTopicsRequest>(&raw).unwrap_or(ActiveTopicsRequest {
                    podcast_ids: Vec::new(),
                    limit: Some(1),
                })
            };
            let allowed_podcasts: HashSet<String> = request.podcast_ids.into_iter().collect();
            let limit = request.limit.unwrap_or(1).max(1).min(MAX_TOPICS);

            let handle_ref = unsafe { &*handle };
            let categories = handle_ref.state.categories.categories_snapshot();
            let projection = match handle_ref.state.library.store.lock() {
                Ok(store) => {
                    let inputs = collect_thread_inputs(&store);
                    let projection = build_projection(inputs.clone(), &categories);
                    build_active_topics(projection, &inputs, &allowed_podcasts, limit)
                }
                Err(_) => return std::ptr::null_mut(),
            };

            match serde_json::to_string(&projection) {
                Ok(json) => match CString::new(json) {
                    Ok(c) => c.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                },
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}

fn collect_thread_inputs(store: &crate::store::PodcastStore) -> Vec<EpisodeThreadInput> {
    let mut inputs = Vec::new();
    for (_podcast, episodes) in store.all_podcasts() {
        for ep in episodes {
            let id = ep.id.0.to_string();
            inputs.push(EpisodeThreadInput {
                podcast_id: ep.podcast_id.0.to_string(),
                episode_id: id.clone(),
                title: ep.title.clone(),
                description: podcast_core::strip_html(&ep.description),
                transcript: store.transcript_for(&id).map(str::to_owned),
                timed_entries: store
                    .timed_transcript_for(&id)
                    .map(|entries| entries.to_vec())
                    .unwrap_or_default(),
                published_at: ep.pub_date.timestamp(),
                played: ep.played,
                triage_archived: ep.triage_decision == Some(TriageDecision::Archived),
            });
        }
    }
    inputs
}

fn build_projection(
    inputs: Vec<EpisodeThreadInput>,
    categories: &HashMap<String, Vec<String>>,
) -> ThreadingProjection {
    let mut by_topic: HashMap<String, Vec<CandidateMention>> = HashMap::new();

    for input in inputs {
        let labels = categories
            .get(&input.episode_id)
            .cloned()
            .unwrap_or_else(|| categorize_text(&input.title, &input.description));
        for label in labels {
            if label.trim().is_empty() {
                continue;
            }
            by_topic
                .entry(label)
                .or_default()
                .push(candidate_for(&input));
        }
    }

    let mut topic_rows: Vec<(i64, ThreadingTopicRow, Vec<ThreadingMentionRow>)> = Vec::new();
    for (label, mut candidates) in by_topic {
        let episode_ids: HashSet<String> = candidates.iter().map(|m| m.episode_id.clone()).collect();
        if episode_ids.len() < MIN_EPISODES_PER_THREAD {
            continue;
        }
        candidates.sort_by(|a, b| {
            b.published_at
                .cmp(&a.published_at)
                .then_with(|| a.episode_id.cmp(&b.episode_id))
        });
        candidates.truncate(MAX_MENTIONS_PER_TOPIC);

        let slug = slugify(&label);
        if slug.is_empty() {
            continue;
        }
        let topic_id = stable_uuid(&format!("thread-topic:{slug}")).to_string();
        let last_mentioned_at = candidates.first().map(|m| m.published_at);
        let topic = ThreadingTopicRow {
            id: topic_id.clone(),
            slug,
            display_name: label.clone(),
            definition: Some(format!(
                "Episodes the kernel categorizer grouped under {label}."
            )),
            episode_mention_count: episode_ids.len(),
            contradiction_count: 0,
            last_mentioned_at,
        };
        let mentions = candidates
            .into_iter()
            .map(|m| ThreadingMentionRow {
                id: stable_uuid(&format!("thread-mention:{topic_id}:{}", m.episode_id)).to_string(),
                topic_id: topic_id.clone(),
                episode_id: m.episode_id,
                start_ms: m.start_ms,
                end_ms: m.end_ms,
                snippet: m.snippet,
                confidence: 0.68,
                is_contradictory: false,
            })
            .collect();
        topic_rows.push((last_mentioned_at.unwrap_or(0), topic, mentions));
    }

    topic_rows.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.display_name.cmp(&b.1.display_name))
    });
    topic_rows.truncate(MAX_TOPICS);

    let mut topics = Vec::new();
    let mut mentions = Vec::new();
    for (_, topic, topic_mentions) in topic_rows {
        topics.push(topic);
        mentions.extend(topic_mentions);
    }
    ThreadingProjection { topics, mentions }
}

fn build_active_topics(
    projection: ThreadingProjection,
    inputs: &[EpisodeThreadInput],
    allowed_podcasts: &HashSet<String>,
    limit: usize,
) -> ActiveTopicsProjection {
    let topic_by_id: HashMap<String, &ThreadingTopicRow> = projection
        .topics
        .iter()
        .map(|topic| (topic.id.clone(), topic))
        .collect();
    let episode_by_id: HashMap<String, &EpisodeThreadInput> = inputs
        .iter()
        .map(|episode| (episode.episode_id.clone(), episode))
        .collect();
    let mut episodes_by_topic: HashMap<String, HashSet<String>> = HashMap::new();
    let mut mentions_by_topic: HashMap<String, Vec<String>> = HashMap::new();

    for mention in projection.mentions {
        let Some(episode) = episode_by_id.get(&mention.episode_id) else {
            continue;
        };
        if episode.played || episode.triage_archived {
            continue;
        }
        if !allowed_podcasts.is_empty() && !allowed_podcasts.contains(&episode.podcast_id) {
            continue;
        }
        episodes_by_topic
            .entry(mention.topic_id.clone())
            .or_default()
            .insert(mention.episode_id.clone());
        mentions_by_topic
            .entry(mention.topic_id)
            .or_default()
            .push(mention.id);
    }

    let mut rows: Vec<ActiveThreadingTopicRow> = episodes_by_topic
        .into_iter()
        .filter_map(|(topic_id, episode_ids)| {
            if episode_ids.len() < MIN_EPISODES_PER_THREAD {
                return None;
            }
            let mention_ids = mentions_by_topic.remove(&topic_id).unwrap_or_default();
            Some(ActiveThreadingTopicRow {
                topic_id,
                unplayed_episode_count: episode_ids.len(),
                mention_ids,
            })
        })
        .collect();

    rows.sort_by(|a, b| {
        let a_topic = topic_by_id.get(&a.topic_id);
        let b_topic = topic_by_id.get(&b.topic_id);
        b.unplayed_episode_count
            .cmp(&a.unplayed_episode_count)
            .then_with(|| {
                b_topic
                    .and_then(|topic| topic.last_mentioned_at)
                    .unwrap_or(0)
                    .cmp(&a_topic.and_then(|topic| topic.last_mentioned_at).unwrap_or(0))
            })
            .then_with(|| {
                a_topic
                    .map(|topic| topic.display_name.as_str())
                    .unwrap_or("")
                    .cmp(b_topic.map(|topic| topic.display_name.as_str()).unwrap_or(""))
            })
    });
    rows.truncate(limit);

    ActiveTopicsProjection {
        active_topics: rows,
    }
}

fn candidate_for(input: &EpisodeThreadInput) -> CandidateMention {
    if let Some(entry) = input
        .timed_entries
        .iter()
        .find(|entry| !entry.text.trim().is_empty())
    {
        return CandidateMention {
            episode_id: input.episode_id.clone(),
            published_at: input.published_at,
            start_ms: seconds_to_ms(entry.start_secs),
            end_ms: seconds_to_ms(entry.end_secs),
            snippet: truncate_snippet(&entry.text),
        };
    }

    let text = input
        .transcript
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&input.description);
    CandidateMention {
        episode_id: input.episode_id.clone(),
        published_at: input.published_at,
        start_ms: 0,
        end_ms: 0,
        snippet: truncate_snippet(text),
    }
}

fn seconds_to_ms(secs: f64) -> i64 {
    if secs.is_finite() && secs > 0.0 {
        (secs * 1000.0).round() as i64
    } else {
        0
    }
}

fn truncate_snippet(text: &str) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.chars().take(220).collect()
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn stable_uuid(seed: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, seed.as_bytes())
}
