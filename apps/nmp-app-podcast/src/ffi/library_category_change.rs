//! Rust-owned category-change planning for agent tools.
//!
//! Swift still persists the legacy category DTO for compatibility, but Rust
//! owns resolving category references, validating podcast/category existence,
//! single-category move semantics, and the kernel label set to write.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct CategoryChangeRequest {
    podcast_id: String,
    #[serde(default)]
    reference: CategoryReference,
    #[serde(default)]
    categories: Vec<CategoryInput>,
}

#[derive(Debug, Default, Deserialize)]
struct CategoryReference {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CategoryInput {
    id: String,
    name: String,
    slug: String,
    #[serde(default)]
    description: String,
    #[serde(default, alias = "colorHex", skip_serializing_if = "Option::is_none")]
    color_hex: Option<String>,
    #[serde(default, alias = "subscriptionIDs")]
    subscription_ids: Vec<String>,
    generated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Debug, Serialize)]
struct CategoryChangeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    categories: Vec<CategoryInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<CategoryChangeResult>,
}

#[derive(Debug, Serialize)]
struct CategoryChangeResult {
    podcast_id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_category_name: Option<String>,
    category_id: String,
    category_name: String,
    category_slug: String,
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
pub extern "C" fn nmp_app_podcast_library_category_change(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_library_category_change",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: CategoryChangeRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&error_response("invalid_request")),
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => plan_change(request, &store),
                Err(_) => error_response("store_unavailable"),
            };
            encode(&response)
        },
    )
}

fn plan_change(
    request: CategoryChangeRequest,
    store: &crate::store::PodcastStore,
) -> CategoryChangeResponse {
    let Ok(podcast_uuid) = uuid::Uuid::parse_str(&request.podcast_id) else {
        return error_response("invalid_podcast_id");
    };
    let podcast_id = podcast_uuid.to_string();
    let Some(podcast) = store.podcast_by_id_str(&podcast_id) else {
        return error_response("missing_podcast");
    };
    let Some(target_index) = resolve_category_index(&request.reference, &request.categories) else {
        return error_response("missing_category");
    };

    let previous = request
        .categories
        .iter()
        .find(|category| category.subscription_ids.iter().any(|id| normalize_uuid(id) == Some(podcast_id.clone())))
        .map(|category| (category.id.clone(), category.name.clone()));

    let mut categories = request.categories;
    for (index, category) in categories.iter_mut().enumerate() {
        category
            .subscription_ids
            .retain(|id| normalize_uuid(id).as_deref() != Some(podcast_id.as_str()));
        if index == target_index {
            category.subscription_ids.push(podcast_id.clone());
        }
    }

    let target = categories[target_index].clone();
    let labels = categories
        .iter()
        .filter(|category| {
            category
                .subscription_ids
                .iter()
                .any(|id| normalize_uuid(id).as_deref() == Some(podcast_id.as_str()))
        })
        .map(|category| category.name.clone())
        .filter(|name| !name.trim().is_empty())
        .collect();

    CategoryChangeResponse {
        error: None,
        categories,
        labels,
        result: Some(CategoryChangeResult {
            podcast_id,
            title: podcast.title.clone(),
            previous_category_id: previous.as_ref().map(|(id, _)| id.clone()),
            previous_category_name: previous.as_ref().map(|(_, name)| name.clone()),
            category_id: target.id,
            category_name: target.name,
            category_slug: target.slug,
        }),
    }
}

fn resolve_category_index(
    reference: &CategoryReference,
    categories: &[CategoryInput],
) -> Option<usize> {
    if let Some(raw_id) = clean(&reference.id) {
        if let Ok(id) = uuid::Uuid::parse_str(&raw_id) {
            let normalized = id.to_string();
            if let Some(index) = categories.iter().position(|category| category.id == normalized) {
                return Some(index);
            }
        }
    }
    if let Some(slug) = clean(&reference.slug) {
        let slug = slug.to_lowercase();
        if let Some(index) = categories.iter().position(|category| category.slug.to_lowercase() == slug) {
            return Some(index);
        }
    }
    if let Some(name) = clean(&reference.name) {
        let name = name.to_lowercase();
        if let Some(index) = categories.iter().position(|category| category.name.to_lowercase() == name) {
            return Some(index);
        }
    }
    None
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_uuid(value: &str) -> Option<String> {
    uuid::Uuid::parse_str(value).ok().map(|uuid| uuid.to_string())
}

fn error_response(error: &str) -> CategoryChangeResponse {
    CategoryChangeResponse {
        error: Some(error.to_string()),
        categories: Vec::new(),
        labels: Vec::new(),
        result: None,
    }
}
