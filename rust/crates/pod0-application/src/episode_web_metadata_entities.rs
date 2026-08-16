use std::sync::LazyLock;

use regex::Regex;

static ENTITY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(#x[0-9A-Fa-f]+|#[0-9]+|[A-Za-z]+);").expect("static entity expression")
});

pub(super) fn decode_html_entities(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    for captures in ENTITY_PATTERN.captures_iter(value) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.push_str(&value[cursor..matched.start()]);
        let replacement = captures
            .get(1)
            .and_then(|body| decoded_entity(body.as_str()));
        result.push_str(replacement.as_deref().unwrap_or(matched.as_str()));
        cursor = matched.end();
    }
    result.push_str(&value[cursor..]);
    result
}

fn decoded_entity(body: &str) -> Option<String> {
    let named = match body {
        "amp" => Some("&"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "nbsp" => Some(" "),
        "mdash" => Some("—"),
        "ndash" => Some("–"),
        "hellip" => Some("…"),
        "lsquo" => Some("‘"),
        "rsquo" => Some("’"),
        "ldquo" => Some("“"),
        "rdquo" => Some("”"),
        _ => None,
    };
    if let Some(named) = named {
        return Some(named.to_owned());
    }
    let scalar = if body.starts_with("#x") || body.starts_with("#X") {
        u32::from_str_radix(&body[2..], 16).ok()
    } else {
        body.strip_prefix('#')?.parse().ok()
    }?;
    char::from_u32(scalar).map(|value| value.to_string())
}
