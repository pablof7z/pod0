//! Small shared helpers for the Podcast FFI surface: a null-aware C-string
//! reader and an HTML-to-plaintext converter for RSS show notes.

use std::ffi::{c_char, CStr};

pub(super) fn c_string_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: caller guarantees `ptr` (when non-null) is a valid
    // nul-terminated C string for the duration of this call.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(std::borrow::ToOwned::to_owned)
}

/// Strip HTML tags and decode common entities from an RSS `<description>`
/// field so the host receives plain text. Both iOS and Android benefit from
/// this at the kernel level (D0 — policy in Rust).
///
/// Strategy: replace each tag with a single space (so `<p>A</p><p>B</p>`
/// → `A  B` rather than `AB`), decode named + numeric entities, then
/// collapse runs of whitespace.
pub(super) fn strip_html(raw: &str) -> String {
    let stripped = strip_tags(raw);
    let decoded = decode_entities(&stripped);
    collapse_whitespace(&decoded)
}

fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => {
                in_tag = true;
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

static NAMED_ENTITIES: &[(&str, &str)] = &[
    ("&amp;", "&"),
    ("&lt;", "<"),
    ("&gt;", ">"),
    ("&quot;", "\""),
    ("&apos;", "'"),
    ("&nbsp;", " "),
    ("&rsquo;", "\u{2019}"),
    ("&lsquo;", "\u{2018}"),
    ("&rdquo;", "\u{201D}"),
    ("&ldquo;", "\u{201C}"),
    ("&hellip;", "\u{2026}"),
    ("&mdash;", "\u{2014}"),
    ("&ndash;", "\u{2013}"),
];

fn decode_entities(input: &str) -> String {
    let mut out = input.to_owned();
    for (entity, replacement) in NAMED_ENTITIES {
        if out.contains(entity) {
            out = out.replace(entity, replacement);
        }
    }
    if out.contains("&#") {
        out = decode_numeric_entities(&out);
    }
    out
}

fn decode_numeric_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' && i + 2 < bytes.len() && bytes[i + 1] == b'#' {
            // Scan up to 12 chars ahead for the closing `;`
            let end = (i + 2..bytes.len().min(i + 14))
                .find(|&j| bytes[j] == b';');
            if let Some(semi) = end {
                let body = &input[i + 2..semi];
                let scalar = if body.starts_with('x') || body.starts_with('X') {
                    u32::from_str_radix(&body[1..], 16).ok()
                } else {
                    body.parse::<u32>().ok()
                };
                if let Some(v) = scalar.and_then(char::from_u32) {
                    out.push(v);
                    i = semi + 1;
                    continue;
                }
            }
        }
        out.push(input[i..].chars().next().unwrap());
        i += input[i..].chars().next().map_or(1, |c| c.len_utf8());
    }
    out
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = true; // trim leading
    for c in input.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    // trim trailing space
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_html;

    #[test]
    fn strips_basic_tags() {
        assert_eq!(strip_html("<p>Hello <b>world</b>.</p>"), "Hello world .");
    }

    #[test]
    fn decodes_named_entities() {
        assert_eq!(strip_html("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(strip_html("it&rsquo;s"), "it\u{2019}s");
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(strip_html("&#39;hello&#39;"), "'hello'");
        assert_eq!(strip_html("&#x2019;"), "\u{2019}");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(strip_html("<p>A</p><p>B</p>"), "A B");
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(strip_html("No tags here."), "No tags here.");
    }

    #[test]
    fn strips_anchor_and_list_tags() {
        let input = r#"<ul><li>Point A</li><li>Point B</li></ul>"#;
        assert_eq!(strip_html(input), "Point A Point B");
    }

    #[test]
    fn mixed_entities_and_tags() {
        let input = "<p>Subscribe at <a href=\"https://ex.com\">our site</a> &amp; enjoy!</p>";
        assert_eq!(strip_html(input), "Subscribe at our site & enjoy!");
    }

    #[test]
    fn newlines_and_tabs_collapsed() {
        let input = "Line 1\n\nLine 2\t\tLine 3";
        assert_eq!(strip_html(input), "Line 1 Line 2 Line 3");
    }
}
