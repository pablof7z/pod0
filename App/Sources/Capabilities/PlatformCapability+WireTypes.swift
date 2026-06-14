// PlatformCapability+WireTypes.swift
//
// WidgetSnapshot, HandoffState, and HandoffUserInfoKey have been moved to the
// codegen pipeline. The canonical definitions now live in:
//
//   App/Sources/Bridge/Generated/PodcastPlatformTypes.generated.swift
//
// Sources of truth (Rust):
//   apps/nmp-app-podcast/src/ffi/projections/platform.rs — WidgetSnapshot
//   apps/podcast-core/src/types/handoff.rs               — HandoffState
//
// Regenerate with:
//   cargo run -p nmp-app-podcast --bin swift-codegen
