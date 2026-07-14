import Foundation

/// Per-subscription auto-download behaviour.
///
/// Mirrors the baseline matrix in `docs/spec/baseline-podcast-features.md` §3:
/// off / latest-N / all-new plus a Wi-Fi-only guard. Modelled as a struct so
/// the mode and the Wi-Fi guard evolve independently.
struct AutoDownloadPolicy: Codable, Sendable, Hashable {
    enum Mode: Codable, Sendable, Hashable {
        /// Never auto-download. User-initiated downloads still allowed.
        case off
        /// Keep the latest N episodes auto-downloaded; older ones may auto-prune.
        case latestN(Int)
        /// Auto-download every new episode the feed reports.
        case allNew
    }

    var mode: Mode
    /// When true, auto-download is gated on the device having a Wi-Fi route.
    var wifiOnly: Bool

    init(mode: Mode = .off, wifiOnly: Bool = true) {
        self.mode = mode
        self.wifiOnly = wifiOnly
    }

    /// Convenience: app default for new subscriptions.
    ///
    /// We default to `.allNew` so a fresh subscription auto-pulls every new
    /// episode the feed reports (Wi-Fi-only). Users who want manual control
    /// can flip the show to `.off` or `.latestN` from Show Detail. This is
    /// the right default for an offline-first listening + transcript app:
    /// nothing else in the agent layer (transcripts, RAG, wiki, briefings)
    /// works well until the audio is local.
    static let `default` = AutoDownloadPolicy(mode: .allNew, wifiOnly: true)

    /// Compact human-readable description used by management surfaces
    /// (Subscriptions list, per-show details). Returns `nil` for the
    /// off case so callers can hide the label entirely instead of
    /// surfacing "Off" noise on every row.
    var summaryLabel: String? {
        switch mode {
        case .off:
            return nil
        case .latestN(let n):
            let base = "Latest \(n)"
            return wifiOnly ? "\(base) · Wi-Fi only" : base
        case .allNew:
            let base = "All new"
            return wifiOnly ? "\(base) · Wi-Fi only" : base
        }
    }
}
