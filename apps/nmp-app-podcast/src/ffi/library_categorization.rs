//! Rust-owned subscription categorization prompt and response parsing.
//!
//! Swift executes the async model call as a capability. Rust owns the prompt,
//! schema, validation, dedupe policy, generated ids, and generated timestamp.

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DESCRIPTION_LIMIT: usize = 600;

#[derive(Debug, Serialize)]
struct CategorizationPromptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CategorizationParseRequest {
    raw_content: String,
}

#[derive(Debug, Deserialize)]
struct ModelResponse {
    categories: Vec<ModelCategory>,
}

#[derive(Debug, Deserialize)]
struct ModelCategory {
    name: String,
    slug: String,
    #[serde(default)]
    description: String,
    #[serde(default, alias = "colorHex")]
    color_hex: Option<String>,
    #[serde(default, alias = "subscriptionIDs")]
    subscription_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CategorizationParseResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    categories: Vec<CategoryRow>,
}

#[derive(Debug, Serialize)]
struct CategoryRow {
    id: String,
    name: String,
    slug: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_hex: Option<String>,
    subscription_ids: Vec<String>,
    generated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

fn encode<T: Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_library_categorization_prompt(
    handle: *mut PodcastHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_library_categorization_prompt",
        std::ptr::null_mut,
        || {
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => build_prompt_response(&store),
                Err(_) => CategorizationPromptResponse {
                    error: Some("store_unavailable".to_string()),
                    model: None,
                    system_prompt: None,
                    user_prompt: None,
                },
            };
            encode(&response)
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_library_categorization_parse(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_library_categorization_parse",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: CategorizationParseRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&parse_error("invalid_request")),
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => parse_response(&request.raw_content, &store),
                Err(_) => parse_error("store_unavailable"),
            };
            encode(&response)
        },
    )
}

fn build_prompt_response(store: &crate::store::PodcastStore) -> CategorizationPromptResponse {
    let model = store.categorization_model().trim().to_string();
    if model.is_empty() {
        return CategorizationPromptResponse {
            error: Some("no_model_selected".to_string()),
            model: None,
            system_prompt: None,
            user_prompt: None,
        };
    }
    let podcasts = followed_podcasts(store);
    if podcasts.is_empty() {
        return CategorizationPromptResponse {
            error: Some("no_subscriptions".to_string()),
            model: Some(model),
            system_prompt: None,
            user_prompt: None,
        };
    }
    CategorizationPromptResponse {
        error: None,
        model: Some(model),
        system_prompt: Some(system_prompt()),
        user_prompt: Some(user_prompt(&podcasts)),
    }
}

fn followed_podcasts(store: &crate::store::PodcastStore) -> Vec<podcast_core::Podcast> {
    let mut podcasts: Vec<_> = store
        .all_podcasts()
        .into_iter()
        .filter(|(podcast, _)| store.is_subscribed(podcast.id))
        .filter(|(podcast, _)| podcast.feed_url.is_some())
        .map(|(podcast, _)| podcast)
        .collect();
    podcasts.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.id.0.to_string().cmp(&b.id.0.to_string()))
    });
    podcasts
}

fn parse_response(raw_content: &str, store: &crate::store::PodcastStore) -> CategorizationParseResponse {
    let decoded: ModelResponse = match serde_json::from_str(&extract_json(raw_content)) {
        Ok(value) => value,
        Err(_) => return parse_error("invalid_response"),
    };
    if decoded.categories.is_empty() {
        return parse_error("invalid_response");
    }

    let valid_ids: HashSet<String> = followed_podcasts(store)
        .into_iter()
        .map(|p| p.id.0.to_string())
        .collect();
    if valid_ids.is_empty() {
        return parse_error("no_subscriptions");
    }

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut built: Vec<CategoryRow> = Vec::with_capacity(decoded.categories.len());
    let generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let model = store.categorization_model().trim();
    let model = (!model.is_empty()).then(|| model.to_string());

    for raw in decoded.categories {
        let mut assigned = Vec::with_capacity(raw.subscription_ids.len());
        for id in raw.subscription_ids {
            let Ok(uuid) = Uuid::parse_str(&id) else {
                return parse_error("invalid_response");
            };
            let normalized = uuid.to_string();
            if !valid_ids.contains(&normalized) {
                return parse_error("invalid_response");
            }
            if let Some(prior_idx) = seen.insert(normalized.clone(), built.len()) {
                built[prior_idx].subscription_ids.retain(|existing| existing != &normalized);
            }
            assigned.push(normalized);
        }
        built.push(CategoryRow {
            id: Uuid::new_v4().to_string(),
            name: raw.name.trim().to_string(),
            slug: raw.slug.trim().to_string(),
            description: raw.description.trim().to_string(),
            color_hex: raw.color_hex.map(|value| value.trim().to_string()),
            subscription_ids: assigned,
            generated_at: generated_at.clone(),
            model: model.clone(),
        });
    }

    built.retain(|category| !category.subscription_ids.is_empty());
    let assigned: HashSet<String> = built
        .iter()
        .flat_map(|category| category.subscription_ids.iter().cloned())
        .collect();
    if assigned != valid_ids {
        return parse_error("invalid_response");
    }

    CategorizationParseResponse {
        error: None,
        categories: built,
    }
}

fn parse_error(error: &str) -> CategorizationParseResponse {
    CategorizationParseResponse {
        error: Some(error.to_string()),
        categories: Vec::new(),
    }
}

fn system_prompt() -> String {
    "You are a podcast librarian. Given a list of podcasts the user follows, group them into 6-12 coherent categories that span the entire library. Return only JSON.\n\nRules:\n- Every podcast must be assigned to exactly one category.\n- Use the exact subscription IDs supplied -- do not invent new IDs.\n- Slug must be lowercase, hyphenated, ASCII (e.g. \"tech-deep-dives\").\n- Description is one short sentence describing what kind of show fits the category.\n- colorHex is optional; when given, use a #RRGGBB tint friendly to a dark, glassy UI.\n- Wrap the entire response in a single ```json``` code fence and do not include any prose outside the fence.".to_string()
}

fn user_prompt(podcasts: &[podcast_core::Podcast]) -> String {
    let mut lines = Vec::with_capacity(podcasts.len() * 5 + 8);
    lines.push("Subscriptions:".to_string());
    for podcast in podcasts {
        lines.push(format!("- id: {}", podcast.id.0));
        lines.push(format!("  title: {}", sanitize(&podcast.title)));
        if !podcast.author.is_empty() {
            lines.push(format!("  author: {}", sanitize(&podcast.author)));
        }
        let description = trim_description(&podcast.description);
        if !description.is_empty() {
            lines.push(format!("  description: {}", sanitize(&description)));
        }
        if !podcast.categories.is_empty() {
            lines.push(format!("  itunes_categories: {}", podcast.categories.join(", ")));
        }
    }
    lines.push(String::new());
    lines.push("Return JSON in this exact shape:".to_string());
    lines.push("```json\n{\n  \"categories\": [\n    {\n      \"name\": \"Display name\",\n      \"slug\": \"display-name\",\n      \"description\": \"One sentence about what fits here.\",\n      \"colorHex\": \"#5B8DEF\",\n      \"subscriptionIDs\": [\"<uuid>\", \"<uuid>\"]\n    }\n  ]\n}\n```".to_string());
    lines.join("\n")
}

fn sanitize(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn trim_description(value: &str) -> String {
    let collapsed = sanitize(value);
    if collapsed.chars().count() <= DESCRIPTION_LIMIT {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(DESCRIPTION_LIMIT).collect();
    out.push_str("...");
    out
}

fn extract_json(content: &str) -> String {
    fenced_substring(content, Some("json"))
        .or_else(|| fenced_substring(content, None))
        .unwrap_or_else(|| content.trim().to_string())
}

fn fenced_substring(content: &str, language: Option<&str>) -> Option<String> {
    let marker = language
        .map(|lang| format!("```{lang}"))
        .unwrap_or_else(|| "```".to_string());
    let start = content.find(&marker)? + marker.len();
    let after_open = &content[start..];
    let newline = after_open.find('\n')?;
    let body_start = start + newline + 1;
    let close = content[body_start..].find("```")? + body_start;
    Some(content[body_start..close].trim().to_string())
}
