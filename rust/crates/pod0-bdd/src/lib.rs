//! `pod0-bdd` — the BDD acceptance layer (`docs/bdd/000-bdd-approach.md`).
//!
//! Test-only: no production crate ever depends on this one. The real entry
//! point is the `harness = false` test target at `tests/bdd/main.rs`, which
//! owns the world and the closed step catalog. This library carries only the
//! cucumber-free fixture builders those steps stage — kept here so they get
//! unit tests under the normal harness and so the test target stays purely
//! about scenario behaviour.

#![forbid(unsafe_code)]

#[cfg(test)]
mod fixtures_tests;

/// The legacy listening-store JSON a fresh install presents to the shared
/// store bootstrap: no subscriptions, no episodes, default settings. The
/// production Swift bootstrap (`SharedLibraryBootstrap.prepare`) feeds the
/// same importer chain from the app's legacy store file; this is that file's
/// empty shape, so a scenario starts from a genuinely clean library.
pub const EMPTY_LEGACY_LISTENING_JSON: &str = r#"{
  "subscriptions": [],
  "episodes": [],
  "settings": {
    "defaultPlaybackRate": 1.0,
    "autoMarkPlayedAtEnd": true,
    "autoPlayNext": false
  }
}"#;

/// One staged episode of a fixture feed: the scenario names only the title;
/// guid, enclosure, and publication date are derived deterministically so a
/// re-run stages byte-identical XML.
#[derive(Debug, Clone)]
pub struct FixtureEpisode {
    pub title: String,
}

/// Render the RSS bytes the scenario's "host" returns for a staged feed.
/// Deliberately minimal but real: this exact XML crosses the facade as a
/// `FeedBytesFetched` observation and is parsed by the production feed
/// normalizer, never by test code.
pub fn rss_feed(podcast_title: &str, episodes: &[FixtureEpisode]) -> String {
    let mut xml = String::from("<rss version=\"2.0\"><channel><title>");
    push_escaped(&mut xml, podcast_title);
    xml.push_str("</title>");
    for (ordinal, episode) in episodes.iter().enumerate() {
        let day = 10 + ordinal;
        xml.push_str("<item><title>");
        push_escaped(&mut xml, &episode.title);
        xml.push_str("</title><guid>");
        push_escaped(&mut xml, &format!("bdd-{}-{ordinal}", slug(podcast_title)));
        xml.push_str("</guid><pubDate>");
        xml.push_str(&format!("Mon, {day} Mar 2025 09:00:00 GMT"));
        xml.push_str("</pubDate><enclosure url=\"https://bdd.example/");
        push_escaped(&mut xml, &format!("{}/{ordinal}.mp3", slug(podcast_title)));
        xml.push_str("\" type=\"audio/mpeg\"/></item>");
    }
    xml.push_str("</channel></rss>");
    xml
}

/// Bytes that are not any podcast feed — what a captive portal or an HTML
/// error page hands the host. The production parser must reject them; the
/// bytes stay obviously non-XML so the scenario cannot accidentally stage a
/// half-valid feed.
pub fn not_a_feed() -> Vec<u8> {
    b"<!doctype html><html><body>router login</body></html>".to_vec()
}

/// Pull every double-quoted name out of a phrase, whatever joins them
/// (`"a", and "b"` / `"a" and "b"` / `"a"`). Shared by every step whose
/// prose lists titles, so scenario text can read the way a product person
/// would write a list.
pub fn parse_quoted_list(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find('"') {
        let Some(length) = rest[start + 1..].find('"') else {
            break;
        };
        out.push(rest[start + 1..start + 1 + length].to_owned());
        rest = &rest[start + length + 2..];
    }
    out
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn push_escaped(xml: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            '"' => xml.push_str("&quot;"),
            _ => xml.push(c),
        }
    }
}
