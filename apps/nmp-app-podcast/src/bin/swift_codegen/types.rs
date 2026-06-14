//! Type manifest — the source-of-truth for the Swift Decodable structs.
//!
//! Each entry here corresponds directly to a Rust struct in
//! `apps/nmp-app-podcast/src/ffi/projections/` or a related source file.
//! Update BOTH the Rust struct AND the manifest entry here when the wire
//! shape changes; the CI drift gate will catch any divergence.

/// How the Swift field value is expressed in the `init(from:)` decoder.
#[derive(Clone, Debug)]
pub enum DecodeStrategy {
    /// `try c.decode(T.self, forKey: .x)` — required field, always present.
    Required,
    /// `try c.decodeIfPresent(T.self, forKey: .x)` — optional, decodes nil.
    Optional,
    /// `try c.decodeIfPresent(T.self, forKey: .x) ?? default` — omitted when default.
    WithDefault(String),
}

/// A single field in a Swift struct.
#[derive(Clone, Debug)]
pub struct Field {
    /// Swift camelCase field name (also the `CodingKeys` case name).
    pub name: &'static str,
    /// Swift type as it appears in the struct body, e.g. `"String"`, `"Int?"`, `"[EpisodeSummary]"`.
    pub swift_type: &'static str,
    /// Default value in the struct declaration, e.g. `"nil"`, `"[]"`, `"false"`, `"0"`.
    /// `None` means the field is a required `let`-style var with no default (the type must be non-optional).
    pub struct_default: Option<&'static str>,
    /// How to emit the decode expression.
    pub decode: DecodeStrategy,
    /// Optional property wrapper, e.g. `"@DefaultFalse"`, `"@DefaultEmptyArray"`.
    pub wrapper: Option<&'static str>,
    /// Optional doc comment line(s) (without `///` prefix).
    pub doc: Option<&'static str>,
    /// Per-field `CodingKeys` raw-value override.
    ///
    /// When `Some(raw)`, the emitted `CodingKeys` case is:
    ///   `case <name> = "<raw>"`
    /// instead of the bare `case <name>` (which uses the Swift case name as the
    /// raw value, i.e. the JSON key the decoder looks for).
    ///
    /// Use this when the JSON key that the decoder sees after applying the
    /// decoder's `keyDecodingStrategy` is neither the Swift field name nor the
    /// camelCase conversion of the Rust snake_case name.  The canonical examples
    /// are `SettingsSnapshot` fields like `ollamaChatURL` (Rust `ollama_chat_url`)
    /// and the credential BYOK ID/label fields whose Swift names use uppercase
    /// acronyms that don't survive the `.convertFromSnakeCase` round-trip.
    ///
    /// **Wire-compatibility note**: the raw value is what the decoder looks for in
    /// the JSON *after* applying the `keyDecodingStrategy`.  With the default
    /// strategy (no conversion) the raw value must match the JSON key verbatim;
    /// with `.convertFromSnakeCase` the strategy converts the JSON key first and
    /// then compares against the raw value.  Always set this to the value that
    /// makes the relevant test suite pass for the decoder configuration in use.
    pub coding_key_override: Option<&'static str>,
}

impl Field {
    pub const fn required(name: &'static str, swift_type: &'static str) -> Self {
        Self { name, swift_type, struct_default: None, decode: DecodeStrategy::Required, wrapper: None, doc: None, coding_key_override: None }
    }
    pub const fn opt(name: &'static str, swift_type: &'static str) -> Self {
        Self { name, swift_type, struct_default: Some("nil"), decode: DecodeStrategy::Optional, wrapper: None, doc: None, coding_key_override: None }
    }
    pub fn default_val(name: &'static str, swift_type: &'static str, default: &'static str) -> Self {
        Self { name, swift_type, struct_default: Some(default), decode: DecodeStrategy::WithDefault(default.to_string()), wrapper: None, doc: None, coding_key_override: None }
    }
    pub fn default_false(name: &'static str) -> Self {
        Self { name, swift_type: "Bool", struct_default: Some("false"), decode: DecodeStrategy::WithDefault("false".to_string()), wrapper: Some("@DefaultFalse"), doc: None, coding_key_override: None }
    }
    pub fn default_empty_array(name: &'static str, elem_type: &'static str) -> Self {
        let swift_type = Box::leak(format!("[{elem_type}]").into_boxed_str());
        Self {
            name,
            swift_type,
            struct_default: Some("[]"),
            decode: DecodeStrategy::WithDefault("[]".to_string()),
            wrapper: Some("@DefaultEmptyArray"),
            doc: None,
            coding_key_override: None,
        }
    }
    pub fn default_empty_strings(name: &'static str) -> Self {
        Self {
            name,
            swift_type: "[String]",
            struct_default: Some("[]"),
            decode: DecodeStrategy::WithDefault("[]".to_string()),
            wrapper: Some("@DefaultEmptyStrings"),
            doc: None,
            coding_key_override: None,
        }
    }
    pub fn with_doc(mut self, doc: &'static str) -> Self {
        self.doc = Some(doc);
        self
    }
    /// Set a per-field `CodingKeys` raw-value override (builder method).
    pub fn with_key(mut self, raw: &'static str) -> Self {
        self.coding_key_override = Some(raw);
        self
    }
}

/// Protocol conformances for a struct.
#[derive(Clone, Debug)]
pub struct Conformances {
    pub identifiable: bool,
    pub equatable: bool,
    pub hashable: bool,
    /// If `identifiable`, the computed property that provides `id`.
    pub id_expr: Option<&'static str>,
}

impl Conformances {
    pub const EQUATABLE: Self = Self { identifiable: false, equatable: true, hashable: false, id_expr: None };
    pub const EQUATABLE_HASHABLE: Self = Self { identifiable: false, equatable: true, hashable: true, id_expr: None };
    pub fn identifiable(id_expr: &'static str) -> Self {
        Self { identifiable: true, equatable: true, hashable: true, id_expr: Some(id_expr) }
    }
    pub const NONE: Self = Self { identifiable: false, equatable: false, hashable: false, id_expr: None };
}

/// Whether the struct gets a hand-written `Codable` extension or a synthesized one.
pub enum CodableKind {
    /// Emit a custom `extension Foo: Codable { init(from:) ... }` block.
    Custom,
    /// The struct already has `Codable` in the protocol list (synthesized).
    Synthesized,
    /// No Codable at all (struct declaration only, extension added manually
    /// or from another source file).
    None,
}

/// A complete struct to emit.
pub struct Struct {
    pub name: &'static str,
    /// Extra protocols beyond auto-selected ones.
    pub extra_protos: &'static [&'static str],
    pub conformances: Conformances,
    pub codable_kind: CodableKind,
    pub fields: Vec<Field>,
    pub doc: Option<&'static str>,
}

// ── Platform type manifests ────────────────────────────────────────────────────
//
// Source of truth for `PodcastPlatformTypes.generated.swift`.
//
// `WidgetSnapshot` — embedded in `PodcastUpdate` (and `WidgetDomainFrame`),
// decoded by the bridge with `.convertFromSnakeCase`.  NO explicit CodingKeys —
// the synthesised camelCase names are exactly what the strategy produces.
// Acronym rule: Rust `artwork_url` → `.convertFromSnakeCase` → `artworkUrl`
// (all-lowercase after the first character of each word), NOT `artworkURL`.
//
// `HandoffState` — constructed in Swift, NOT decoded from Rust JSON via the
// bridge decoder. Carries explicit snake_case `CodingKeys` for the two fields
// whose Swift names use uppercase-acronym suffixes (`episodeID`, `podcastID`)
// that `.convertFromSnakeCase` would map differently (`Id` vs `ID`). The
// `coding_key_override` mechanic documents this explicitly.
//
// Source of truth:
//   apps/nmp-app-podcast/src/ffi/projections/platform.rs — WidgetSnapshot
//   apps/podcast-core/src/types/handoff.rs               — HandoffState

/// Manifest for `WidgetSnapshot`.
///
/// All fields are camelCase (synthesised keys, no explicit CodingKeys).
/// Decoded embedded in `PodcastUpdate` via `.convertFromSnakeCase`.
pub fn widget_snapshot_fields() -> Vec<Field> {
    vec![
        Field::opt("nowPlayingEpisodeTitle", "String")
            .with_doc("Title of the active episode, when one is loaded."),
        Field::opt("nowPlayingPodcastTitle", "String")
            .with_doc("Title of the podcast/show the active episode belongs to."),
        Field::opt("nowPlayingArtworkUrl", "String")
            .with_doc("Artwork URL (episode-level preferred, falls back to show). \
                       Rust: `now_playing_artwork_url` → camelCase `nowPlayingArtworkUrl` \
                       (NOT `artworkURL` — `.convertFromSnakeCase` lowercases acronyms)."),
        Field::opt("nowPlayingChapterTitle", "String")
            .with_doc("Active chapter title at the playhead; nil for chapter-less episodes."),
        Field::default_val("isPlaying", "Bool", "false")
            .with_doc("`true` while playback is engaged."),
        Field::default_val("positionFraction", "Float", "0")
            .with_doc("Pre-computed progress fraction 0.0..=1.0."),
        Field::default_val("positionSecs", "Double", "0")
            .with_doc("Current playhead in seconds."),
        Field::default_val("durationSecs", "Double", "0")
            .with_doc("Track duration in seconds; 0 until reported."),
        Field::default_val("unplayedCount", "Int", "0")
            .with_doc("Unplayed episode count across subscribed shows."),
    ]
}

/// Manifest for `HandoffState`.
///
/// Uses explicit `CodingKeys` for the two fields whose Swift names use
/// uppercase-acronym suffixes (`episodeID`, `podcastID`) that the
/// `.convertFromSnakeCase` strategy would otherwise map to `Id` (lowercase d).
/// Source: apps/podcast-core/src/types/handoff.rs.
pub fn handoff_state_fields() -> Vec<Field> {
    vec![
        Field::required("activityType", "String")
            .with_doc("`io.f7z.podcast.playing` or `io.f7z.podcast.browsing`."),
        Field::opt("episodeID", "String")
            .with_key("episode_id")
            .with_doc("Episode identifier; present for the `playing` activity."),
        Field::opt("podcastID", "String")
            .with_key("podcast_id")
            .with_doc("Podcast identifier; present for `browsing` activity."),
        Field::opt("positionSecs", "Double")
            .with_key("position_secs")
            .with_doc("Playhead position in seconds; present when activity is `playing`."),
    ]
}
