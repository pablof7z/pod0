import Foundation
import SwiftUI

// MARK: - DiagnosticLevel

/// Severity of a single diagnostic entry. Drives the color-coded badge in
/// `DebugLogsView` and lets the log be scanned for problems at a glance.
enum DiagnosticLevel: String, CaseIterable, Sendable {
    case debug
    case info
    case warning
    case error

    /// Short uppercase tag rendered in the row badge.
    var label: String {
        switch self {
        case .debug:   return "DEBUG"
        case .info:    return "INFO"
        case .warning: return "WARN"
        case .error:   return "ERROR"
        }
    }

    /// Badge tint. Maps to the UI spec: debug=gray, info=blue,
    /// warning=orange, error=red. Routed through `AppTheme.Tint` where a
    /// token exists so a palette tweak lands in one place.
    var tint: Color {
        switch self {
        case .debug:   return AppTheme.Tint.dimmed
        case .info:    return .blue
        case .warning: return AppTheme.Tint.warning
        case .error:   return AppTheme.Tint.error
        }
    }
}

// MARK: - DiagnosticEntry

/// One captured log line. Value type so the ring buffer can be snapshotted
/// for the SwiftUI list without locking.
struct DiagnosticEntry: Identifiable, Sendable {
    let id: UUID
    let timestamp: Date
    let level: DiagnosticLevel
    let category: String
    let message: String
}

// MARK: - DiagnosticLog

/// In-memory ring buffer of recent diagnostic events, surfaced under
/// Settings → Debug → Logs.
///
/// Design contract:
///   - **Off by default.** `isEnabled` is backed by `UserDefaults`
///     (`debugLoggingEnabled`, default `false`). When disabled,
///     `append(...)` is a genuine no-op.
///   - **Zero overhead when disabled.** `message` is an `@autoclosure`, so
///     the interpolation at each call site (e.g. the per-tick kernel
///     snapshot string + its `library.reduce`) is never evaluated unless
///     logging is on. This matters: the tap points sit on documented hot
///     paths (kernel projection tick, download report emit).
///   - **Bounded.** At most `capacity` (500) entries are retained; the
///     oldest are dropped first.
///
/// `@MainActor`-isolated: every tap point already runs on the main actor
/// (kernel observation Task, `scenePhase` switch, `DownloadCapability.emit`,
/// kernel dispatch helpers), so no hopping is required at any call site.
@MainActor
@Observable
final class DiagnosticLog {
    static let shared = DiagnosticLog()

    /// Maximum retained entries. Oldest are evicted once exceeded.
    static let capacity = 500

    /// UserDefaults key for the persisted on/off toggle.
    static let enabledDefaultsKey = "debugLoggingEnabled"

    /// Captured entries, oldest-first. UI renders newest-first.
    private(set) var entries: [DiagnosticEntry] = []

    /// Whether capture is active. Persisted to `UserDefaults`; flipping it
    /// off leaves already-captured entries in place (the user can still
    /// read what was collected before disabling).
    var isEnabled: Bool {
        didSet {
            guard isEnabled != oldValue else { return }
            UserDefaults.standard.set(isEnabled, forKey: Self.enabledDefaultsKey)
        }
    }

    private init() {
        self.isEnabled = UserDefaults.standard.bool(forKey: Self.enabledDefaultsKey)
    }

    /// Append a diagnostic entry. No-op (and the `message` closure is never
    /// evaluated) when logging is disabled.
    func append(
        level: DiagnosticLevel,
        category: String,
        message: @autoclosure () -> String
    ) {
        guard isEnabled else { return }
        entries.append(DiagnosticEntry(
            id: UUID(),
            timestamp: Date(),
            level: level,
            category: category,
            message: message()
        ))
        // Ring-buffer eviction. Drop from the front so the newest 500 are
        // retained. removeFirst(_:) on a small overflow (we append one at a
        // time) is effectively O(1) amortized here.
        if entries.count > Self.capacity {
            entries.removeFirst(entries.count - Self.capacity)
        }
    }

    /// Drop every captured entry. Leaves `isEnabled` untouched.
    func clear() {
        entries.removeAll()
    }

    /// Render all entries as plain text, newest-first, for the clipboard.
    /// One line per entry: `HH:mm:ss.SSS  LEVEL  [category]  message`.
    func plainText() -> String {
        entries
            .reversed()
            .map { entry in
                let ts = Self.clipboardFormatter.string(from: entry.timestamp)
                return "\(ts)  \(entry.level.label)  [\(entry.category)]  \(entry.message)"
            }
            .joined(separator: "\n")
    }

    /// `HH:mm:ss.SSS` — millisecond precision so closely-spaced ticks stay
    /// distinguishable. Shared by the clipboard export; the row view formats
    /// its own to keep this type UI-agnostic.
    static let clipboardFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()
}
