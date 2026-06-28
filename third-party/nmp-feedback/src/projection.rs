use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const KIND_TEXT_NOTE: u32 = 1;
const KIND_METADATA: u32 = 513;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FeedbackReplyDto {
    pub event_id: String,
    pub author_pubkey: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FeedbackThreadDto {
    pub event_id: String,
    pub author_pubkey: String,
    pub category: String,
    pub content: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_label: Option<String>,
    pub replies: Vec<FeedbackReplyDto>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RawEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    pubkey: String,
    #[serde(default)]
    created_at: i64,
    #[serde(default)]
    kind: u32,
    #[serde(default)]
    tags: Vec<Vec<String>>,
    #[serde(default)]
    content: String,
}

impl RawEvent {
    fn a_tags(&self) -> impl Iterator<Item = &str> {
        self.tags
            .iter()
            .filter(|tag| tag.len() >= 2 && tag[0] == "a")
            .map(|tag| tag[1].as_str())
    }

    fn first_e_tag(&self) -> Option<&str> {
        self.tags
            .iter()
            .find(|tag| tag.len() >= 2 && tag[0] == "e")
            .map(|tag| tag[1].as_str())
    }

    fn root_event_id(&self) -> Option<&str> {
        if let Some(marked) = self
            .tags
            .iter()
            .find(|tag| tag.len() >= 4 && tag[0] == "e" && tag[3] == "root")
        {
            return Some(marked[1].as_str());
        }
        self.first_e_tag()
    }

    fn category(&self) -> String {
        let tagged = self
            .tags
            .iter()
            .find(|tag| tag.len() >= 2 && (tag[0] == "t" || tag[0] == "category"))
            .map(|tag| tag[1].to_lowercase());
        match tagged.as_deref() {
            Some("bug") => "bug",
            Some("feature-request") | Some("feature request") => "feature-request",
            Some("question") => "question",
            Some("praise") => "praise",
            _ => "bug",
        }
        .to_string()
    }
}

#[derive(Clone, Debug, Default)]
struct MetaParsed {
    created_at: i64,
    title: Option<String>,
    summary: Option<String>,
    status_label: Option<String>,
}

pub fn reduce_feedback_threads(
    events: &[Value],
    project_coordinate: &str,
) -> Vec<FeedbackThreadDto> {
    let parsed: Vec<RawEvent> = events
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .filter(|event: &RawEvent| !event.id.is_empty())
        .collect();

    let mut meta_by_root: HashMap<String, MetaParsed> = HashMap::new();
    for event in parsed.iter().filter(|event| event.kind == KIND_METADATA) {
        let Some(root) = event.root_event_id() else {
            continue;
        };
        match meta_by_root.get(root) {
            Some(existing) if existing.created_at >= event.created_at => continue,
            _ => {
                meta_by_root.insert(root.to_string(), parse_meta(event));
            }
        }
    }

    let mut replies_by_root: HashMap<String, Vec<&RawEvent>> = HashMap::new();
    for event in parsed
        .iter()
        .filter(|event| event.kind == KIND_TEXT_NOTE && event.root_event_id().is_some())
    {
        let root = event.root_event_id().unwrap().to_string();
        replies_by_root.entry(root).or_default().push(event);
    }

    let mut roots: Vec<&RawEvent> = parsed
        .iter()
        .filter(|event| {
            event.kind == KIND_TEXT_NOTE
                && event.root_event_id().is_none()
                && event.a_tags().any(|coord| coord == project_coordinate)
        })
        .collect();
    roots.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    roots
        .into_iter()
        .map(|root| {
            let mut replies = replies_by_root.get(&root.id).cloned().unwrap_or_default();
            replies.sort_by(|left, right| left.created_at.cmp(&right.created_at));
            let meta = meta_by_root.get(&root.id);
            FeedbackThreadDto {
                event_id: root.id.clone(),
                author_pubkey: root.pubkey.clone(),
                category: root.category(),
                content: root.content.clone(),
                created_at: root.created_at,
                title: meta.and_then(|m| m.title.clone()),
                summary: meta.and_then(|m| m.summary.clone()),
                status_label: meta.and_then(|m| m.status_label.clone()),
                replies: replies
                    .into_iter()
                    .map(|reply| FeedbackReplyDto {
                        event_id: reply.id.clone(),
                        author_pubkey: reply.pubkey.clone(),
                        content: reply.content.clone(),
                        created_at: reply.created_at,
                    })
                    .collect(),
            }
        })
        .collect()
}

fn parse_meta(event: &RawEvent) -> MetaParsed {
    let mut title = None;
    let mut summary = None;
    let mut status = None;
    for tag in &event.tags {
        if tag.len() < 2 {
            continue;
        }
        match tag[0].as_str() {
            "title" => title.get_or_insert_with(|| tag[1].clone()),
            "summary" => summary.get_or_insert_with(|| tag[1].clone()),
            "status-label" | "status_label" | "status" => {
                status.get_or_insert_with(|| tag[1].clone())
            }
            _ => continue,
        };
    }
    if (title.is_none() || summary.is_none() || status.is_none()) && !event.content.is_empty() {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&event.content) {
            let string_value = |key: &str| {
                map.get(key)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            };
            title = title.or_else(|| string_value("title"));
            summary = summary.or_else(|| string_value("summary"));
            status = status
                .or_else(|| string_value("status_label"))
                .or_else(|| string_value("status"));
        }
    }
    MetaParsed {
        created_at: event.created_at,
        title,
        summary,
        status_label: status,
    }
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
