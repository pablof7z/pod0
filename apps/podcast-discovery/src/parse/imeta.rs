//! `imeta` tag parsing helpers (NIP-92).
//!
//! Split out from `parse/episode.rs` so the file-LOC budget stays under
//! AGENTS.md's 300-LOC soft limit. The `imeta` payload is generic across
//! kinds — moving it here keeps the episode parser readable and lets
//! future kinds (e.g. NIP-94 audio uploads) reuse the helper.

/// Fields extracted from a single `imeta` tag.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImetaFields {
    pub url: Option<String>,
    pub mime: Option<String>,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

/// Parse the trailing components of an `imeta` tag.
///
/// Per NIP-92, the `imeta` payload after the tag name is a sequence of
/// space-separated `key value` pairs; the publisher emits `url`, `m`,
/// `x`, `size`, and optionally `duration`. We tolerate extra trailing
/// content by taking the first whitespace token as the key and the rest
/// of the line as the value (matches Swift `hasPrefix("url ")` parsing).
pub(crate) fn parse_imeta_fields(imeta: &[String]) -> ImetaFields {
    let mut out = ImetaFields::default();
    for entry in imeta.iter().skip(1) {
        let (key, value) = match entry.split_once(' ') {
            Some(parts) => parts,
            None => continue,
        };
        match key {
            "url" => out.url = Some(value.to_string()),
            "m" => out.mime = Some(value.to_string()),
            "x" => out.sha256 = Some(value.to_string()),
            "size" => out.size = value.parse().ok(),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_imeta_returns_default() {
        let fields = parse_imeta_fields(&["imeta".into()]);
        assert_eq!(fields, ImetaFields::default());
    }

    #[test]
    fn parses_publisher_emitted_block() {
        let imeta = vec![
            "imeta".into(),
            "url https://media.example/ep.m4a".into(),
            "m audio/mp4".into(),
            "x deadbeef".into(),
            "size 1234".into(),
        ];
        let fields = parse_imeta_fields(&imeta);
        assert_eq!(fields.url.as_deref(), Some("https://media.example/ep.m4a"));
        assert_eq!(fields.mime.as_deref(), Some("audio/mp4"));
        assert_eq!(fields.sha256.as_deref(), Some("deadbeef"));
        assert_eq!(fields.size, Some(1234));
    }

    #[test]
    fn ignores_unknown_keys_and_malformed_entries() {
        let imeta = vec![
            "imeta".into(),
            "url https://x".into(),
            "no-space-entry".into(), // dropped — no ' ' delimiter
            "blurhash abcdef".into(), // unknown key — ignored
        ];
        let fields = parse_imeta_fields(&imeta);
        assert_eq!(fields.url.as_deref(), Some("https://x"));
        assert!(fields.mime.is_none());
    }

    #[test]
    fn size_parse_failure_yields_none() {
        let imeta = vec!["imeta".into(), "size not-a-number".into()];
        let fields = parse_imeta_fields(&imeta);
        assert!(fields.size.is_none());
    }
}
