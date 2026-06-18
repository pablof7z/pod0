import Foundation

// MARK: - AppStateStore + Position (render-only)
//
// The Rust kernel is the single source of truth for playback position.
// `audio_report.rs::apply_writeback` updates `ep.position_secs` on each
// `Playing` tick and flushes to disk on pause/stop/sleep/end and on a
// ~10 s playhead delta (`POSITION_FLUSH_DELTA_SECS`).
//
// Swift never *originates* a position write. It consumes position in two
// render-only ways:
//
//   • `kernel.nowPlaying.positionSecs` — the live playhead for the scrubber.
//   • `episode.playbackPosition` — the kernel-persisted resume point shown on
//     the episode row, projected from `ep.position_secs`; `episode(id:)`
//     applies the live kernel value as a display-only floor.
//
// The App Group SQLite episode store still persists the kernel-sourced
// position as a display mirror (so the row renders before the kernel
// projection arrives). That mirror is a cache, not a second source — its
// removal is tracked as a follow-up #561 seam, at which point this file
// (which now holds no code) is deleted.
//
// `--UITestSeedRelaunch` preserves the kernel's `podcasts.json` and wipes the
// SQLite mirror, proving resume survives a cold restart from the kernel alone.
