// EpisodeFormatHelpers.swift
// Canonical free-function formatters shared across all episode row and player
// views. Having one implementation eliminates ~15 private copies that were
// duplicated verbatim across Library, Player, Inbox, and Briefings surfaces.

import Foundation

/// Format `secs` as `M:SS` or `H:MM:SS`. Returns `"--:--"` for non-finite or negative input.
func formatDuration(_ secs: Double) -> String {
    guard secs.isFinite, secs >= 0 else { return "--:--" }
    let total = Int(secs.rounded())
    let h = total / 3600
    let m = (total % 3600) / 60
    let s = total % 60
    if h > 0 { return String(format: "%d:%02d:%02d", h, m, s) }
    return String(format: "%d:%02d", m, s)
}

/// Format a Unix timestamp as an abbreviated relative string (e.g. "2h ago", "3d ago").
func relativeDate(from unixSeconds: Int) -> String {
    relativeDate(from: Date(timeIntervalSince1970: TimeInterval(unixSeconds)))
}

/// Format a `Date` as an abbreviated relative string (e.g. "2h ago", "3d ago").
func relativeDate(from date: Date) -> String {
    _sharedRelativeFormatter.localizedString(for: date, relativeTo: Date())
}

nonisolated(unsafe) private let _sharedRelativeFormatter: RelativeDateTimeFormatter = {
    let f = RelativeDateTimeFormatter()
    f.unitsStyle = .abbreviated
    return f
}()
