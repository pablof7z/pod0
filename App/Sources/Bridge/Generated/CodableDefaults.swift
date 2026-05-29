// CodableDefaults.swift
// Property wrappers that let the hand-maintained projection mirrors decode the
// Rust kernel's D5 "omit-on-empty / omit-when-false" wire shape WITHOUT making
// every field Optional (and without rippling `?? default` into every reader).
//
// Why this exists: the Rust projection types use `#[serde(skip_serializing_if]`
// to omit empty collections / false bools / default settings from the snapshot
// wire (D5 byte-identity). Swift's *synthesized* `Decodable` calls
// `decode(_:forKey:)` for a non-optional stored property, which throws
// `keyNotFound` when the key is absent — so a non-optional `[T]` / `Bool` mirror
// field fails to decode the moment the kernel omits it (which is the common
// case). These wrappers override `KeyedDecodingContainer.decode(_:forKey:)` for
// the wrapper type so an absent key resolves to the default instead of throwing.
// The wrapper is transparent at the use site (`summary.played` still reads a
// plain `Bool`), so no call site changes.

import Foundation

// MARK: - Defaultable

/// A type with a canonical "absent on the wire" default.
protocol CodableDefaultSource {
    associatedtype Value: Codable & Equatable & Hashable
    static var defaultValue: Value { get }
}

/// Generic default-on-absence wrapper. Decodes the underlying value when the
/// key is present; falls back to `Source.defaultValue` when absent or null.
@propertyWrapper
struct CodableDefault<Source: CodableDefaultSource>: Codable, Equatable, Hashable {
    var wrappedValue: Source.Value

    init(wrappedValue: Source.Value) { self.wrappedValue = wrappedValue }

    init(from decoder: Decoder) throws {
        // Decode strictly: a PRESENT-but-malformed value must throw (don't mask
        // schema drift). The missing/null-key default is applied by the
        // `KeyedDecodingContainer.decode(_:forKey:)` overload below, which only
        // substitutes the default when `decodeIfPresent` reports the key absent.
        let container = try decoder.singleValueContainer()
        wrappedValue = try container.decode(Source.Value.self)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(wrappedValue)
    }
}

extension KeyedDecodingContainer {
    /// Called by synthesized `Decodable` for a `@CodableDefault` property. An
    /// absent/null key resolves to the source default; a present-but-invalid
    /// value still throws (via the wrapper's strict `init(from:)`).
    func decode<Source>(
        _ type: CodableDefault<Source>.Type, forKey key: Key
    ) throws -> CodableDefault<Source> {
        try decodeIfPresent(type, forKey: key)
            ?? CodableDefault(wrappedValue: Source.defaultValue)
    }
}

// MARK: - Default sources

enum BoolFalse: CodableDefaultSource { static let defaultValue = false }

enum EmptyStringArray: CodableDefaultSource { static let defaultValue: [String] = [] }

/// Convenience aliases used by the projection mirrors.
typealias DefaultFalse = CodableDefault<BoolFalse>
typealias DefaultEmptyStrings = CodableDefault<EmptyStringArray>

// MARK: - Generic empty-array wrapper

/// Default-empty wrapper for arbitrary `Codable & Equatable & Hashable` element
/// arrays (e.g. `[EpisodeSummary]`, `[InboxItem]`). Decodes `[]` when the key is
/// absent. Separate from `CodableDefault` because the element type is the
/// generic parameter (no per-type `CodableDefaultSource` needed).
@propertyWrapper
struct DefaultEmptyArray<Element: Codable>: Codable {
    var wrappedValue: [Element]

    init(wrappedValue: [Element]) { self.wrappedValue = wrappedValue }

    init(from decoder: Decoder) throws {
        // Strict: present-but-malformed throws; the empty-default for an absent
        // key is applied by the `decode(_:forKey:)` overload below.
        let container = try decoder.singleValueContainer()
        wrappedValue = try container.decode([Element].self)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(wrappedValue)
    }
}

// Conditional conformances so a wrapped property doesn't block the host struct's
// synthesized `Equatable`/`Hashable` when the element supports them.
extension DefaultEmptyArray: Equatable where Element: Equatable {}
extension DefaultEmptyArray: Hashable where Element: Hashable {}

extension KeyedDecodingContainer {
    func decode<Element>(
        _ type: DefaultEmptyArray<Element>.Type, forKey key: Key
    ) throws -> DefaultEmptyArray<Element> {
        try decodeIfPresent(type, forKey: key) ?? DefaultEmptyArray(wrappedValue: [])
    }
}

// MARK: - SettingsSnapshot default

/// Default-on-absence wrapper for `SettingsSnapshot`. The Rust projection omits
/// `settings` when it equals the default (D5: `SettingsSnapshot::is_default`), so
/// an absent key resolves to a fresh `SettingsSnapshot()`. Only requires
/// `Codable` (SettingsSnapshot is not `Hashable`), so it suits `PodcastUpdate`
/// which is `Codable`-only.
@propertyWrapper
struct DefaultSettings: Codable {
    var wrappedValue: SettingsSnapshot

    init(wrappedValue: SettingsSnapshot) { self.wrappedValue = wrappedValue }

    init(from decoder: Decoder) throws {
        // Strict: present-but-malformed throws; the default for an absent key is
        // applied by the `decode(_:forKey:)` overload below.
        let container = try decoder.singleValueContainer()
        wrappedValue = try container.decode(SettingsSnapshot.self)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(wrappedValue)
    }
}

extension KeyedDecodingContainer {
    func decode(_ type: DefaultSettings.Type, forKey key: Key) throws -> DefaultSettings {
        try decodeIfPresent(type, forKey: key) ?? DefaultSettings(wrappedValue: SettingsSnapshot())
    }
}
