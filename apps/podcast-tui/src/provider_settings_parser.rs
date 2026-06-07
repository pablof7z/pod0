use std::env;

use crate::runtime::Result;

pub(crate) fn parse_model_input(input: &str) -> Result<(String, String)> {
    let (model, name) = match input.split_once('|') {
        Some((model, name)) => (model.trim(), name.trim()),
        None => (input.trim(), ""),
    };
    if model.is_empty() {
        return Err("model id is required".to_owned());
    }
    Ok((model.to_owned(), name.to_owned()))
}

pub(crate) fn parse_credential_input(
    input: &str,
) -> Result<(String, Option<String>, Option<String>, Option<i64>)> {
    let parts = input.split('|').map(str::trim).collect::<Vec<_>>();
    let source = parts.first().copied().unwrap_or_default();
    if matches!(source, "" | "none" | "null" | "clear" | "off") {
        return Ok((String::new(), None, None, None));
    }
    let connected_at = parts
        .get(3)
        .copied()
        .and_then(optional_string)
        .map(|value| parse_connected_at(&value))
        .transpose()?;
    Ok((
        source.to_owned(),
        parts.get(1).copied().and_then(optional_string),
        parts.get(2).copied().and_then(optional_string),
        connected_at,
    ))
}

pub(crate) fn parse_connected_at(input: &str) -> Result<i64> {
    if input.eq_ignore_ascii_case("now") {
        return Ok(chrono::Utc::now().timestamp());
    }
    input
        .parse::<i64>()
        .map_err(|_| "connected_at must be epoch seconds or now".to_owned())
}

pub(crate) fn parse_required_pair(
    input: &str,
    left_label: &str,
    right_label: &str,
) -> Result<(String, String)> {
    let Some((left, right)) = input.split_once('|') else {
        return Err(format!("format: {left_label} | {right_label}"));
    };
    let left = require_nonempty(left, left_label)?;
    let right = require_nonempty(right, right_label)?;
    Ok((left, right))
}

pub(crate) fn parse_pair_allow_blank(input: &str) -> (String, String) {
    match input.split_once('|') {
        Some((left, right)) => (left.trim().to_owned(), right.trim().to_owned()),
        None => (input.trim().to_owned(), String::new()),
    }
}

pub(crate) fn parse_provider_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn require_nonempty(input: &str, label: &str) -> Result<String> {
    let value = input.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value.to_owned())
    }
}

pub(crate) fn optional_string(input: &str) -> Option<String> {
    let value = input.trim();
    if value.is_empty() || matches!(value, "none" | "null" | "clear") {
        None
    } else {
        Some(value.to_owned())
    }
}

pub(crate) fn credential_input(
    source: &str,
    key_id: Option<&str>,
    key_label: Option<&str>,
    connected_at: Option<i64>,
) -> String {
    format!(
        "{} | {} | {} | {}",
        source,
        key_id.unwrap_or_default(),
        key_label.unwrap_or_default(),
        connected_at
            .map(|value| value.to_string())
            .unwrap_or_default()
    )
}

pub(crate) fn credential_summary(
    source: &str,
    key_id: Option<&str>,
    key_label: Option<&str>,
    connected_at: Option<i64>,
) -> String {
    if source.is_empty() {
        return "none".to_owned();
    }
    let mut parts = vec![source.to_owned()];
    if let Some(label) = key_label.filter(|value| !value.is_empty()) {
        parts.push(label.to_owned());
    } else if let Some(id) = key_id.filter(|value| !value.is_empty()) {
        parts.push(id.to_owned());
    }
    if let Some(ts) = connected_at {
        parts.push(ts.to_string());
    }
    parts.join(" ")
}

pub(crate) fn env_credentials_summary() -> String {
    let keys = [
        ("OPENROUTER_API_KEY", env_key("OPENROUTER_API_KEY")),
        ("OLLAMA_API_KEY", env_key("OLLAMA_API_KEY")),
        ("ELEVENLABS_API_KEY", env_key("ELEVENLABS_API_KEY")),
        ("ASSEMBLYAI_API_KEY", env_key("ASSEMBLYAI_API_KEY")),
        ("PERPLEXITY_API_KEY", env_key("PERPLEXITY_API_KEY")),
    ];
    let present = keys
        .iter()
        .filter_map(|(name, value)| value.as_ref().map(|_| *name))
        .collect::<Vec<_>>();
    if present.is_empty() {
        "no env keys".to_owned()
    } else {
        present.join(", ")
    }
}

pub(crate) fn env_key(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .and_then(|value| optional_string(&value))
}

pub(crate) fn model_input(model: &str, model_name: &str) -> String {
    format!("{model} | {model_name}")
}

pub(crate) fn model_summary(model: &str, model_name: &str) -> String {
    if model_name.is_empty() {
        model.to_owned()
    } else {
        format!("{model_name} ({model})")
    }
}

pub(crate) fn pair_summary(left: &str, right: &str) -> String {
    if left.is_empty() && right.is_empty() {
        "none".to_owned()
    } else {
        format!("{left} | {right}")
    }
}

pub(crate) fn bool_label(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_input_accepts_optional_display_name() {
        assert_eq!(
            parse_model_input("openrouter:openai/gpt-4o | GPT-4o").unwrap(),
            ("openrouter:openai/gpt-4o".to_owned(), "GPT-4o".to_owned())
        );
        assert_eq!(
            parse_model_input("local:gemma4-e2b").unwrap(),
            ("local:gemma4-e2b".to_owned(), String::new())
        );
    }

    #[test]
    fn credential_input_can_clear_or_parse_metadata() {
        assert_eq!(
            parse_credential_input("clear").unwrap(),
            (String::new(), None, None, None)
        );
        assert_eq!(
            parse_credential_input("byok | key-1 | Work key | 1710000000").unwrap(),
            (
                "byok".to_owned(),
                Some("key-1".to_owned()),
                Some("Work key".to_owned()),
                Some(1_710_000_000)
            )
        );
    }

    #[test]
    fn provider_list_discards_empty_segments() {
        assert_eq!(
            parse_provider_list("elevenlabs_scribe, ,assemblyai"),
            vec!["elevenlabs_scribe".to_owned(), "assemblyai".to_owned()]
        );
    }

    #[test]
    fn optional_model_blank_clears() {
        assert_eq!(optional_string(" "), None);
        assert_eq!(optional_string("none"), None);
        assert_eq!(
            optional_string("local:gemma"),
            Some("local:gemma".to_owned())
        );
    }
}
