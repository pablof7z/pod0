// PerfSignposter.swift
// Shared os_signpost / OSSignposter handles for measuring the kernel-projection
// hot paths with Instruments (Time Profiler + os_signpost instrument).
//
// MEASUREMENT ONLY — these symbols add no logic. They exist so that
// `KernelModel.applyPodcastUpdate`, `AppStateStore.applyKernelState` /
// `toEpisode`, `AppStateStore.recomputeEpisodeProjections`, and
// `HomeView.triageCounts` can emit signpost intervals under one subsystem.
//
// Subsystem: `com.podcastr.perf`. Open Instruments, add the "os_signpost"
// instrument, and filter on this subsystem to read per-operation wall-clock
// (begin/end interval) durations and counts.
//
// IMPORTANT: `perfLog` and `signposter` are module-INTERNAL (not `private`).
// Swift has no per-symbol import within a module, so internal access lets the
// four consumer files reference these directly. Files that call the C
// `os_signpost(_:log:name:_:)` API only need `import os`.

import os
import os.signpost

/// Shared `OSLog` handle for the perf subsystem. Used both by the
/// `OSSignposter` below and by direct C `os_signpost(.begin/.end, log:…)`
/// calls (e.g. `recomputeEpisodeProjections`).
let perfLog = OSLog(subsystem: "com.podcastr.perf", category: "kernel-projection")

/// Shared signposter for the perf subsystem. `beginInterval` / `endInterval`
/// callers across the projection hot paths emit through this single handle so
/// Instruments groups them under one subsystem.
let signposter = OSSignposter(logHandle: perfLog)
