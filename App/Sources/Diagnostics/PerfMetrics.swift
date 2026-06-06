import Foundation

// MARK: - PerfOp

/// The instrumented hot paths. Each case names one segment of the kernel/FFI
/// round-trip we want per-operation cost on. The split mirrors the actual
/// thread boundaries so the UI can separate *background* FFI cost from
/// *main-thread* cost (the latter is what actually locks the UI):
///
///   • `pushFrameDecode` — Rust actor thread. FlatBuffer→JSON + envelope JSON
///     parse for every kernel push frame (up to `emitHz`, 4 Hz during playback).
///   • `mainApply` — main actor. `KernelModel.apply` segment: identity diff,
///     spotlight index, live-activity + now-playing reconcile.
///   • `mainProjection` — main actor. `AppStateStore.applyKernelState`: the
///     O(N) library→AppState projection (full path) or its snapshot-only fast
///     path.
///   • `dispatchAction` — caller thread (usually main). A synchronous
///     `nmp_app_dispatch_action` FFI round-trip plus its post-dispatch pull.
///   • `snapshotPull` — caller thread (usually main). A full-library
///     `nmp_app_podcast_snapshot` serialize + decode.
enum PerfOp: String, CaseIterable, Sendable {
    case pushFrameDecode
    case mainApply
    case mainProjection
    case dispatchAction
    case snapshotPull

    /// Human-readable label for the Performance view rows.
    var title: String {
        switch self {
        case .pushFrameDecode: return "Push-frame decode"
        case .mainApply:       return "Main · apply"
        case .mainProjection:  return "Main · projection"
        case .dispatchAction:  return "FFI · dispatch"
        case .snapshotPull:    return "FFI · snapshot pull"
        }
    }

    /// Whether this op carries a payload byte count worth surfacing.
    var tracksBytes: Bool {
        switch self {
        case .pushFrameDecode, .snapshotPull: return true
        case .mainApply, .mainProjection, .dispatchAction: return false
        }
    }

    /// Whether this op runs on the main actor (so its cost is UI-blocking).
    var isMainThread: Bool {
        switch self {
        case .mainApply, .mainProjection: return true
        case .pushFrameDecode, .dispatchAction, .snapshotPull: return false
        }
    }
}

// MARK: - Stat value types

/// Aggregated cost for one `PerfOp`. Value type so `snapshot()` hands the UI an
/// immutable copy with no shared mutable state.
struct PerfOpStat: Sendable {
    var count: Int = 0
    var totalMicros: Int = 0
    var maxMicros: Int = 0
    var totalBytes: Int = 0

    var avgMicros: Int { count == 0 ? 0 : totalMicros / count }
}

/// Main-thread responsiveness summary, driven by `MainThreadWatchdog`. Every
/// probe sample is folded in here; `jankCount`/`hangCount` bucket the samples
/// that exceeded the thresholds.
struct WatchdogStat: Sendable {
    var sampleCount: Int = 0
    var jankCount: Int = 0
    var hangCount: Int = 0
    var maxStallMillis: Double = 0
    var lastStallMillis: Double = 0
    var lastStallAt: Date?
}

/// Immutable copy of the collector handed to the UI on each refresh.
struct PerfMetricsSnapshot: Sendable {
    var ops: [PerfOp: PerfOpStat]
    var watchdog: WatchdogStat
    var since: Date
    var elapsed: TimeInterval
}

// MARK: - PerfMetrics

/// Thread-safe, lock-guarded performance collector for the kernel/FFI bridge.
///
/// Touched from BOTH the Rust actor thread (`pushFrameDecode` recorded in the
/// C update callback) AND the main actor (`mainApply` / `mainProjection`), so
/// it is deliberately NOT `@MainActor`-isolated — it serializes every mutation
/// behind a single `NSLock`. The traffic is low (≤ 4 Hz frames + a 20 Hz
/// watchdog), so the lock is uncontended and cheap; a lock-free design would be
/// over-engineering.
///
/// Design contract (mirrors `DiagnosticLog`):
///   - **Off by default.** `isEnabled` is `UserDefaults`-backed
///     (`perfMetricsEnabled`). When off, `record` is a no-op and the
///     watchdog timer is not running — genuine zero overhead.
///   - Flipping it on starts the `MainThreadWatchdog`; flipping it off stops it
///     and leaves the already-collected stats in place to read.
final class PerfMetrics: @unchecked Sendable {
    static let shared = PerfMetrics()

    /// UserDefaults key for the persisted on/off toggle.
    static let enabledDefaultsKey = "perfMetricsEnabled"

    /// A main-thread latency sample at or above this is counted as "jank"
    /// (a couple of dropped frames). Below it the main thread is responsive.
    static let jankThresholdMillis: Double = 80
    /// At or above this the sample is counted as a "hang" — a stall a user
    /// perceives as the UI freezing.
    static let hangThresholdMillis: Double = 250

    private let lock = NSLock()
    private var ops: [PerfOp: PerfOpStat] = [:]
    private var watchdog = WatchdogStat()
    private var since = Date()
    private var _enabled: Bool

    private let watchdogEngine = MainThreadWatchdog()

    /// Whether collection (and the watchdog timer) is active.
    var isEnabled: Bool {
        lock.lock(); defer { lock.unlock() }
        return _enabled
    }

    private init() {
        _enabled = UserDefaults.standard.bool(forKey: Self.enabledDefaultsKey)
        if _enabled { startWatchdog() }
    }

    /// Toggle collection. Persists the flag and starts/stops the watchdog.
    func setEnabled(_ on: Bool) {
        lock.lock()
        let changed = on != _enabled
        _enabled = on
        lock.unlock()
        guard changed else { return }
        UserDefaults.standard.set(on, forKey: Self.enabledDefaultsKey)
        if on { startWatchdog() } else { watchdogEngine.stop() }
    }

    // MARK: Recording

    /// Fold one timed sample into `op`. No-op when disabled. Cheap enough to sit
    /// on the 4 Hz push-frame and main-projection paths.
    func record(_ op: PerfOp, micros: Int, bytes: Int = 0) {
        lock.lock()
        guard _enabled else { lock.unlock(); return }
        var stat = ops[op] ?? PerfOpStat()
        stat.count += 1
        stat.totalMicros += micros
        if micros > stat.maxMicros { stat.maxMicros = micros }
        stat.totalBytes += bytes
        ops[op] = stat
        lock.unlock()
    }

    /// Fold one main-thread responsiveness sample (from the watchdog) into the
    /// watchdog summary. `millis` is how long the main queue took to service a
    /// trivial probe block — i.e. the main-thread stall for that sample.
    private func recordLatencySample(millis: Double) {
        lock.lock()
        guard _enabled else { lock.unlock(); return }
        watchdog.sampleCount += 1
        if millis >= Self.hangThresholdMillis {
            watchdog.hangCount += 1
        } else if millis >= Self.jankThresholdMillis {
            watchdog.jankCount += 1
        }
        if millis > watchdog.maxStallMillis { watchdog.maxStallMillis = millis }
        watchdog.lastStallMillis = millis
        watchdog.lastStallAt = Date()
        lock.unlock()
    }

    // MARK: Read / reset

    /// Immutable copy for the UI. Cheap; safe to call from a 1 Hz refresh timer.
    func snapshot() -> PerfMetricsSnapshot {
        lock.lock(); defer { lock.unlock() }
        return PerfMetricsSnapshot(
            ops: ops, watchdog: watchdog,
            since: since, elapsed: Date().timeIntervalSince(since))
    }

    /// Clear all counters and restart the measurement window. Leaves
    /// `isEnabled` (and the watchdog) untouched.
    func reset() {
        lock.lock()
        ops = [:]
        watchdog = WatchdogStat()
        since = Date()
        lock.unlock()
    }

    // MARK: Watchdog wiring

    private func startWatchdog() {
        watchdogEngine.start(intervalMillis: 50) { [weak self] millis in
            self?.recordLatencySample(millis: millis)
        }
    }
}
