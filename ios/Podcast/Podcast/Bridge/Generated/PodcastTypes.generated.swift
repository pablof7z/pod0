// PodcastTypes.generated.swift
// Generated — do not hand-edit. Regenerate via:
//
//   cargo run -p nmp-app-podcast --features codegen-schema \
//       --bin dump_projection_schemas \
//     | cargo run -p nmp-codegen -- gen swift
//
// Source of truth: apps/podcast/nmp-app-podcast/src/ffi/snapshot.rs

import Foundation

/// Top-level snapshot emitted by the Rust podcast kernel on every tick.
/// M0 stub — fields will grow as Rust projection modules are implemented.
struct PodcastUpdate: Codable {
    var running: Bool = false
    var rev: Int = 0
}
