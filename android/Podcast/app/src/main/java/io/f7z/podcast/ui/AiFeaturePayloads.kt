package io.f7z.podcast.ui

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Action payloads for AI chapter synthesis and ad-skip settings.
 *
 * Split from `ActionDispatcher.kt` to keep that file under the 500-line hard
 * limit (AGENTS.md). All wire contracts verified against the Rust action enums.
 */

// ── `podcast.chapters` namespace payloads ─────────────────────────────────
//
// Verified against `apps/nmp-app-podcast/src/ffi/actions/chapters_module.rs`
// `ChaptersAction` enum:
//
//   ChaptersAction::Compile { episode_id: String }  → op = "compile"
//
// Namespace is `"podcast.chapters"` (ChaptersActionModule::NAMESPACE = "podcast.chapters").

/**
 * Trigger AI chapter synthesis for an episode that has a cached transcript
 * but no RSS / Podcasting 2.0 chapters yet. Verified against
 * `ChaptersAction::Compile { episode_id }`.
 *
 * The kernel returns `{"ok":true,"status":"compiling","episode_id":…}` and
 * projects the synthesized chapters onto the next `EpisodeSummary.chapters`
 * push frame. No local state — the shell renders whatever the snapshot reports.
 */
@Serializable
data class CompileChaptersPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "compile",
)

// ── `podcast.settings` ad-skip payload ───────────────────────────────────
//
// Verified against `apps/nmp-app-podcast/src/ffi/actions/settings_module.rs`
// `SettingsAction` enum:
//
//   SettingsAction::SetAutoSkipAds { enabled: bool }  → op = "set_auto_skip_ads"
//
// The current toggle state is projected via `SettingsSnapshot.auto_skip_ads_enabled`.

/**
 * Toggle the auto-skip-ads player feature. Verified against
 * `SettingsAction::SetAutoSkipAds { enabled: bool }`.
 *
 * When enabled, the kernel's `PlayerActor` seeks past each `AdSegment`
 * detected in `EpisodeSummary.ad_segments`. The current value is projected
 * via `SettingsSnapshot.auto_skip_ads_enabled`.
 */
@Serializable
data class SetAutoSkipAdsPayload(
    val enabled: Boolean,
    val op: String = "set_auto_skip_ads",
)
