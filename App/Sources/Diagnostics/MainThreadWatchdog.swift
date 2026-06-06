import Foundation

/// Background-driven main-thread responsiveness probe.
///
/// The discriminating instrument for "why does the UI feel sluggish": it does
/// not need to know *what* blocks the main thread. From a utility background
/// queue it posts a trivial block to the main queue and measures how long the
/// main queue takes to service it. That latency IS the main-thread stall for
/// that sample — whatever the cause (the kernel projection, Spotlight indexing,
/// SwiftUI layout, an unrelated `sync`). `PerfMetrics` buckets the samples into
/// jank / hang counts.
///
/// **Single-outstanding-ping design.** The next probe is scheduled only after
/// the current one is serviced. A naive fixed-interval timer would, during a
/// 400 ms hang, pile up ~8 pending main blocks that all resolve at once —
/// distorting the sample count and adding main-thread load precisely when it is
/// already saturated. Here there is never more than one probe in flight, so the
/// instrument cannot itself contribute to the stall it measures.
///
/// All mutable state (`running`, `intervalMillis`, `onSample`) is touched ONLY
/// on `queue`, so no additional locking is needed — the serial queue is the
/// synchronization. The probe's main-thread block does no mutation; it measures,
/// then hops back to `queue` to invoke the callback and re-arm.
final class MainThreadWatchdog: @unchecked Sendable {
    private let queue = DispatchQueue(label: "perf.mainthread.watchdog", qos: .utility)
    private var running = false
    private var intervalMillis: Int = 50
    private var onSample: ((Double) -> Void)?

    /// Start probing. `onSample` is invoked (on the watchdog queue) once per
    /// probe with the main-thread service latency in milliseconds. Idempotent —
    /// a second `start` while already running is ignored.
    func start(intervalMillis: Int, onSample: @escaping @Sendable (Double) -> Void) {
        queue.async { [weak self] in
            guard let self, !self.running else { return }
            self.running = true
            self.intervalMillis = intervalMillis
            self.onSample = onSample
            self.scheduleNext()
        }
    }

    /// Stop probing. The in-flight probe (if any) completes and then the loop
    /// halts; `onSample` is released so the collector can be torn down cleanly.
    func stop() {
        queue.async { [weak self] in
            self?.running = false
            self?.onSample = nil
        }
    }

    private func scheduleNext() {
        queue.asyncAfter(deadline: .now() + .milliseconds(intervalMillis)) { [weak self] in
            guard let self, self.running else { return }
            // `postedAt` is captured on the watchdog queue immediately before the
            // hop, so the measured latency is exactly the main queue's
            // service delay for this block — the main-thread stall.
            let postedAt = DispatchTime.now().uptimeNanoseconds
            DispatchQueue.main.async {
                let latencyNanos = DispatchTime.now().uptimeNanoseconds &- postedAt
                let millis = Double(latencyNanos) / 1_000_000
                // Hop back to the watchdog queue: invoke the callback and re-arm
                // there so `onSample`/`running` are only ever read off-main.
                self.queue.async { [weak self] in
                    guard let self, self.running else { return }
                    self.onSample?(millis)
                    self.scheduleNext()
                }
            }
        }
    }
}
