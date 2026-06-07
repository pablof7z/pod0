//! Podcast-domain tools the agent can call during a chat turn (M5.4).
//!
//! Rather than lean on rig-core's `#[derive(Tool)]` machinery — which assumes
//! the provider speaks OpenAI-style structured tool calls — this module
//! implements a small, model-agnostic tool layer. Local models (deepseek via
//! Ollama) are far more reliable at emitting a single JSON object than at the
//! provider-native function-calling protocol, so [`crate::agent_llm`] drives a
//! manual loop: it asks the model to reply with `{"tool":...,"args":{...}}`,
//! [`parse_tool_call`] extracts that, and [`ToolRegistry::execute`] runs it
//! against the shared [`PodcastStore`].
//!
//! All tools operate on the **string** form of UUIDs (matching the
//! `id.0.to_string()` convention used throughout `store/`), so the agent can
//! round-trip ids it discovered via `search_library` without us parsing them
//! back into typed ids.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde_json::Value;

use crate::inbox_llm::TriageResult;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::PodcastStore;

/// Maximum transcript characters returned by `get_transcript`. Keeps a single
/// tool result from blowing past the model's context window.
const TRANSCRIPT_CHAR_LIMIT: usize = 2000;

/// Number of search hits surfaced by `search_library`.
const SEARCH_RESULT_LIMIT: usize = 5;

/// A parsed tool call extracted from a model response.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub args: Value,
}

/// Human-readable description block injected into the system prompt so the
/// model knows which tools exist and how to invoke them.
pub const TOOL_INSTRUCTIONS: &str = "\
Use a tool only when needed. Tool call format, with no extra text: \
{\"tool\":\"<name>\",\"args\":{...}}\n\
Available tools: search_library {\"query\":\"text\"}; get_transcript \
{\"episode_id\":\"uuid\"}; get_podcast_info {\"podcast_id\":\"uuid\"}; \
get_memory_facts {}.\n\
After a tool result, answer the original question in plain text. If no tool is \
needed, answer normally.";

/// Tool instructions for the background inbox-triage agent. Restricted to
/// read tools + the batch write tool; no transcript/mutating tools exposed.
pub const TRIAGE_TOOL_INSTRUCTIONS: &str = "\
Use tools by replying with only JSON: {\"tool\":\"<name>\",\"args\":{...}}\n\
Available tools: get_memory_facts {}; search_library {\"query\":\"text\"}; \
set_episode_priorities {\"scores\":[{\"episode_id\":\"uuid\",\"score\":0.0,\
\"reason\":\"one sentence\",\"categories\":[\"tag\"]}]}.\n\
For triage, inspect memory/library if useful, then call set_episode_priorities \
once with every listed episode. Do not answer in plain text.";

/// State held by [`ToolRegistry`] when operating in triage mode.
struct TriageSink {
    cache: Arc<Mutex<HashMap<String, TriageResult>>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
}

/// Holds the shared store and executes named tool calls against it.
pub struct ToolRegistry {
    store: Arc<Mutex<PodcastStore>>,
    triage: Option<TriageSink>,
}

impl ToolRegistry {
    /// Chat path — no triage write access.
    pub fn new(store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            store,
            triage: None,
        }
    }

    /// Triage path — gains `set_episode_priorities` write access.
    pub fn for_triage(
        store: Arc<Mutex<PodcastStore>>,
        cache: Arc<Mutex<HashMap<String, TriageResult>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self {
            store,
            triage: Some(TriageSink {
                cache,
                rev,
                snapshot_signal: None,
            }),
        }
    }

    pub fn for_triage_with_signal(
        store: Arc<Mutex<PodcastStore>>,
        cache: Arc<Mutex<HashMap<String, TriageResult>>>,
        rev: Arc<AtomicU64>,
        snapshot_signal: SnapshotUpdateSignal,
    ) -> Self {
        Self {
            store,
            triage: Some(TriageSink {
                cache,
                rev,
                snapshot_signal: Some(snapshot_signal),
            }),
        }
    }

    /// Execute a named tool with the given JSON args, returning a plain-text
    /// result to feed back to the model. Never panics: every error path
    /// returns a descriptive string the model can reason about.
    pub fn execute(&self, tool: &str, args: &Value) -> String {
        match tool {
            "search_library" => self.search_library(args),
            "get_transcript" => self.get_transcript(args),
            "get_podcast_info" => self.get_podcast_info(args),
            "get_memory_facts" => self.get_memory_facts(),
            "set_episode_priorities" => self.set_episode_priorities(args),
            other => format!("unknown tool: {other}"),
        }
    }

    fn search_library(&self, args: &Value) -> String {
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if query.is_empty() {
            return "search_library: missing 'query' argument".to_owned();
        }
        let needle = query.to_lowercase();

        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return "search_library: store unavailable".to_owned(),
        };

        let mut hits: Vec<String> = Vec::new();
        for (podcast, episodes) in store.subscribed_podcasts() {
            let podcast_id = podcast.id.0.to_string();
            for ep in episodes {
                let matches = ep.title.to_lowercase().contains(&needle)
                    || ep.description.to_lowercase().contains(&needle)
                    || podcast.title.to_lowercase().contains(&needle)
                    || podcast.author.to_lowercase().contains(&needle);
                if matches {
                    hits.push(format!(
                        "- \"{}\" (podcast: \"{}\", episode_id: {}, podcast_id: {})",
                        ep.title, podcast.title, ep.id.0, podcast_id
                    ));
                    if hits.len() >= SEARCH_RESULT_LIMIT {
                        break;
                    }
                }
            }
            if hits.len() >= SEARCH_RESULT_LIMIT {
                break;
            }
        }

        if hits.is_empty() {
            format!("No matches found for \"{query}\".")
        } else {
            format!("Found {} match(es):\n{}", hits.len(), hits.join("\n"))
        }
    }

    fn get_transcript(&self, args: &Value) -> String {
        let episode_id = args
            .get("episode_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if episode_id.is_empty() {
            return "get_transcript: missing 'episode_id' argument".to_owned();
        }

        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return "get_transcript: store unavailable".to_owned(),
        };

        match store.transcript_for(episode_id) {
            Some(text) => {
                let truncated: String = text.chars().take(TRANSCRIPT_CHAR_LIMIT).collect();
                if text.chars().count() > TRANSCRIPT_CHAR_LIMIT {
                    format!("{truncated}\n…[transcript truncated]")
                } else {
                    truncated
                }
            }
            None => "no transcript available for that episode".to_owned(),
        }
    }

    /// Return every stored memory fact as a plain-text `key: value` list, or a
    /// clear "none stored" message. Takes no args (M5.6).
    fn get_memory_facts(&self) -> String {
        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return "get_memory_facts: store unavailable".to_owned(),
        };

        let facts = store.all_memory_facts();
        if facts.is_empty() {
            return "No memory facts stored.".to_owned();
        }

        facts
            .iter()
            .map(|f| format!("{}: {}", f.key, f.value))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn get_podcast_info(&self, args: &Value) -> String {
        let podcast_id = args
            .get("podcast_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if podcast_id.is_empty() {
            return "get_podcast_info: missing 'podcast_id' argument".to_owned();
        }

        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return "get_podcast_info: store unavailable".to_owned(),
        };

        let Some(podcast) = store.podcast_by_id_str(podcast_id) else {
            return format!("no podcast found with id {podcast_id}");
        };

        let episodes = store.episodes_for(podcast.id);
        let episode_count = episodes.len();
        let last_published = episodes
            .iter()
            .map(|e| e.pub_date)
            .max()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_owned());

        format!(
            "Title: {}\nAuthor: {}\nEpisode count: {episode_count}\nLatest episode published: {last_published}",
            podcast.title,
            if podcast.author.is_empty() {
                "unknown"
            } else {
                &podcast.author
            },
        )
    }

    /// Write a batch of episode priority scores into the triage cache.
    ///
    /// Only available when the registry was constructed via [`Self::for_triage`].
    /// Parses the `scores` array tolerantly — bad entries are skipped so a
    /// partially-malformed reply still records the valid scores. Bumps `rev`
    /// once after all valid entries are written.
    fn set_episode_priorities(&self, args: &Value) -> String {
        let sink = match &self.triage {
            Some(s) => s,
            None => return "set_episode_priorities: not available in chat mode".to_owned(),
        };

        let scores = match args.get("scores").and_then(Value::as_array) {
            Some(arr) => arr,
            None => return "set_episode_priorities: missing 'scores' array".to_owned(),
        };

        let now = Utc::now().timestamp();
        let mut written = 0usize;

        if let Ok(mut cache) = sink.cache.lock() {
            for entry in scores {
                let ep_id = match entry.get("episode_id").and_then(Value::as_str) {
                    Some(id) if !id.is_empty() => id.to_owned(),
                    _ => continue,
                };
                let score = entry.get("score").and_then(Value::as_f64).unwrap_or(0.5) as f32;
                let reason = entry
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("Agent-scored episode")
                    .to_owned();
                let categories = entry
                    .get("categories")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();

                cache.insert(
                    ep_id,
                    TriageResult::ready(score.clamp(0.0, 1.0), reason, categories, now),
                );
                written += 1;
            }
        }

        if written > 0 {
            if let Some(signal) = &sink.snapshot_signal {
                signal.bump();
            } else {
                sink.rev.fetch_add(1, Ordering::Relaxed);
            }
        }

        format!("Recorded {written} score(s).")
    }
}

/// Try to extract a tool call from a model response.
///
/// Returns `Some(ToolCall)` when the response contains a JSON object with a
/// string `tool` field and an `args` object. Tolerant of common local-model
/// quirks: surrounding prose and ```json fences are stripped by scanning for
/// the first balanced `{...}` object. Returns `None` for plain prose so the
/// caller treats the response as a final answer.
pub fn parse_tool_call(response: &str) -> Option<ToolCall> {
    let candidate = extract_json_object(response)?;
    let value: Value = serde_json::from_str(&candidate).ok()?;
    let name = value.get("tool")?.as_str()?.to_owned();
    if name.is_empty() {
        return None;
    }
    // Default to an empty object when args is omitted so tools can report
    // their own missing-argument errors rather than failing to parse.
    let args = value
        .get("args")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    Some(ToolCall { name, args })
}

/// Parse a tool call for the background triage path.
///
/// This accepts the generic `{"tool":"set_episode_priorities","args":{...}}`
/// wrapper and the typed direct payload `{"scores":[...]}`. The direct shape is
/// still normalized to the same backend write tool; it is not accepted by
/// general chat.
pub fn parse_triage_tool_call(response: &str) -> Option<ToolCall> {
    let candidate = extract_json_object(response)?;
    let value: Value = serde_json::from_str(&candidate).ok()?;

    if let Some(tool) = value.get("tool").and_then(Value::as_str) {
        if tool == "set_episode_priorities" {
            if let Some(args) = value
                .get("args")
                .filter(|args| args.get("scores").is_some())
            {
                return Some(ToolCall {
                    name: tool.to_owned(),
                    args: args.clone(),
                });
            }
            if value.get("scores").is_some() {
                return scores_tool_call(&value);
            }
        }
        return parse_tool_call(response);
    }

    scores_tool_call(&value)
}

fn scores_tool_call(value: &Value) -> Option<ToolCall> {
    let scores = value.get("scores")?;
    if !scores.is_array() {
        return None;
    }
    let mut args = serde_json::Map::new();
    args.insert("scores".to_owned(), scores.clone());
    Some(ToolCall {
        name: "set_episode_priorities".to_owned(),
        args: Value::Object(args),
    })
}

/// Scan `text` for the first balanced top-level JSON object and return it as a
/// slice. Handles braces inside string literals (and escaped quotes) so a
/// transcript value containing `{` doesn't desync the depth counter.
fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for i in start..bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..=i].to_owned());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[path = "agent_tools_tests.rs"]
mod tests;
