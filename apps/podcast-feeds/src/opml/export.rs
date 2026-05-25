use chrono::{DateTime, Utc};
use podcast_core::Podcast;

/// Produces an OPML 2.0 document from the user's current subscriptions.
/// Ports `OPMLExport.swift`. Output shape mirrors Apple Podcasts / Pocket
/// Casts so re-importing into another app is lossless.
///
/// Synthetic podcasts (no `feed_url`) are skipped — nothing to round-trip.
pub fn export_opml(podcasts: &[Podcast]) -> String {
    export_opml_with(
        podcasts,
        "Podcastr Subscriptions",
        Utc::now(),
    )
}

/// Test-friendly variant with explicit title and timestamp.
pub fn export_opml_with(
    podcasts: &[Podcast],
    title: &str,
    date_created: DateTime<Utc>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string());
    lines.push("<opml version=\"2.0\">".to_string());
    lines.push("  <head>".to_string());
    lines.push(format!("    <title>{}</title>", escape(title)));
    lines.push(format!(
        "    <dateCreated>{}</dateCreated>",
        rfc822(date_created)
    ));
    lines.push("  </head>".to_string());
    lines.push("  <body>".to_string());
    lines.push("    <outline text=\"feeds\" title=\"feeds\">".to_string());

    for podcast in podcasts {
        if let Some(line) = outline_for(podcast) {
            lines.push(line);
        }
    }

    lines.push("    </outline>".to_string());
    lines.push("  </body>".to_string());
    lines.push("</opml>".to_string());

    lines.join("\n")
}

fn outline_for(podcast: &Podcast) -> Option<String> {
    let feed_url = podcast.feed_url.as_ref()?;
    let mut attrs: Vec<(&str, String)> = vec![
        ("type", "rss".to_string()),
        ("text", podcast.title.clone()),
        ("title", podcast.title.clone()),
        ("xmlUrl", feed_url.as_str().to_string()),
    ];
    if !podcast.description.is_empty() {
        attrs.push(("description", podcast.description.clone()));
    }
    if let Some(lang) = podcast.language.as_deref() {
        if !lang.is_empty() {
            attrs.push(("language", lang.to_string()));
        }
    }
    let rendered = attrs
        .into_iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape(&v)))
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("      <outline {rendered} />"))
}

/// Minimal XML attribute escaping. Covers the five XML predefined entities
/// plus CR/LF fold so attribute values stay on one line.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

fn rfc822(date: DateTime<Utc>) -> String {
    date.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}
