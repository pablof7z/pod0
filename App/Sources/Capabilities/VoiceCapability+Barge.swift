import Foundation

// MARK: - Barge-in (policy migrated to Rust)
//
// The barge-in cancellation policy moved to the Rust kernel in PR #567
// (feat/550-voice-mode). The kernel's voice manager watches the partial
// transcript stream and emits `VoiceCommand::Stop` when a non-empty
// partial arrives while TTS is active. The iOS capability executes the
// Stop command via `commandHandler` in VoiceCapability+Wire.swift.
//
// This file is kept as a compile-time placeholder because the Xcode
// project still references it. The previous Swift barge-in body has
// been removed — all policy lives in Rust (D0).
