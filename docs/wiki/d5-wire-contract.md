---
title: D5 Wire Contract and Swift Decode Resilience
slug: d5-wire-contract
summary: The Rust D5 wire contract omits default-valued fields; Swift mirrors must decode absent keys with defaults via property wrappers.
tags:
  - d5
  - wire-contract
  - serialization
  - codable
  - schema
volatility: cold
confidence: medium
created: 2026-05-29
updated: 2026-05-29
verified: 2026-05-29
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
---

# D5 Wire Contract and Swift Decode Resilience

> The Rust D5 wire contract omits default-valued fields; Swift mirrors must decode absent keys with defaults via property wrappers.

## D5 Wire Contract

The Rust projection types use `#[serde(skip_serializing_if)]` annotations to omit fields from the wire when they equal their default value. This is the D5 byte-identity contract:
- `#[serde(default, skip_serializing_if = "std::ops::Not::not")]` on `bool` fields — omitted when `false`
- `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on `Vec<T>` fields — omitted when empty
- `#[serde(default, skip_serializing_if = "Option::is_none")]` on `Option<T>` fields — omitted when `None`

This contract is enforced by 31 Rust tests asserting byte-identity (e.g., `default_snapshot_omits_now_playing`, `omits_empty_*`). The D5 contract must be preserved — always-serializing to work around Swift decoder limitations is not permitted. <!-- [^14943-12] -->

## Swift Default-Tolerant Decoding

Swift's synthesized `Codable` ignores property defaults (`= false`, `= []`) for absent keys. When Rust omits a field that the Swift mirror types non-optionally, the decoder throws `keyNotFound`. The fix is property wrappers that provide defaults when the key is absent, combined with `KeyedDecodingContainer.decode(_:forKey:)` overloads:

- `@CodableDefault<Source>` — generic wrapper with a `DefaultValue` source type
- `@DefaultFalse` — decodes `Bool`, defaulting to `false` on missing/null
- `@DefaultEmptyStrings` — decodes `[String]`, defaulting to `[]` on missing/null
- `@DefaultEmptyArray<Element: Codable>` — decodes `[Element]`, defaulting to `[]` on missing/null
- `@DefaultSettings` — decodes `SettingsSnapshot`, defaulting to `.init()` on missing/null

These wrappers are strict: present-but-malformed values throw (the decoder calls `try container.decode(...)`, not `try?`). Only missing keys and null values receive the default. <!-- [^14943-13] -->

## Affected Fields

The D5-vs-non-optional mismatch affects many fields across the type tree. Fields whose Swift mirrors are non-optional but Rust skips-when-default:

**PodcastSummary:** `autoDownload` (bool, skip-when-false), `episodes` (array, skip-when-empty)

**EpisodeSummary:** `played` (bool, skip-when-false), `starred` (bool, skip-when-false), `aiCategories` (array, skip-when-empty), `adSegments` (array, skip-when-empty)

**InboxItem:** `aiCategories` (array, skip-when-empty)

**PodcastUpdate:** `library`, `searchResults`, `nostrResults`, `comments`, `queue`, `picks`, `inbox`, `nowPlaying`, `downloads`, `tasks`, `knowledge`, `notifications`, and other top-level arrays (all skip-when-empty); `settings` (struct, skip-when-none)

All these fields use the appropriate default-tolerant wrapper in the Swift mirror. Fields whose Swift mirrors are already Optional are unaffected. <!-- [^14943-14] -->

## Systemic Nature

This is a systemic schema-mirror drift — not a one-off field mismatch. Because the pull path uses the same Swift decoder with `try?` (silently falling back to empty), real-data library decode was broken on both push and pull prior to the fix. The `try?` hid the failures, so empty-state frames appeared to work while any frame containing real podcast data failed silently. <!-- [^14943-15] -->

## Codex-Enforced

The review gate flagged the initial always-serialize approach as a P1: it broke the 31 D5 contract tests. The correct fix is to preserve the D5 wire contract intact on the Rust side and make the Swift mirror decode missing keys with defaults. <!-- [^14943-16] -->

## See Also
- [[nmp-update-transport|NMP Update Transport (FlatBuffers Push)]] — related guide
- [[codex-review-gate|Codex Review Gate]] — related guide
- [[known-bug-patterns|Known Bug Patterns]] — related guide

