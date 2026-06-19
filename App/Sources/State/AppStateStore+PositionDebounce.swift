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
// The App Group SQLite episode store does NOT persist position (#561). Episode
// blobs load with playbackPosition=0; the kernel projection (first snapshot
// after launch) immediately fills in the kernel-authoritative value from
// podcasts.json. `--UITestSeedRelaunch` preserves podcasts.json and wipes
// SQLite, proving resume-across-restart is driven solely by the kernel.
