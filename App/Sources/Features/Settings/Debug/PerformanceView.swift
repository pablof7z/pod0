import SwiftUI

// MARK: - PerformanceView
//
// Settings → Debug → Performance. A live HUD over `PerfMetrics`: the
// main-thread stall watchdog summary (the "why is the UI sluggish" signal) plus
// per-operation FFI / main-thread cost. Off by default — flip "Capture metrics"
// on, reproduce the sluggishness, then read the numbers here.
//
// The collector is a plain lock-guarded type (touched from the Rust actor
// thread AND main), not `@Observable`, so this view pulls a fresh immutable
// `snapshot()` on a 1 Hz timer rather than observing it.

struct PerformanceView: View {
    @State private var enabled = PerfMetrics.shared.isEnabled
    @State private var snapshot = PerfMetrics.shared.snapshot()
    @State private var showResetConfirmation = false

    private let refresh = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        Form {
            captureSection
            if enabled || snapshot.watchdog.sampleCount > 0 {
                watchdogSection
                operationsSection
                resetSection
            }
        }
        .navigationTitle("Performance")
        .navigationBarTitleDisplayMode(.inline)
        .onReceive(refresh) { _ in snapshot = PerfMetrics.shared.snapshot() }
        .confirmationDialog(
            "Reset metrics?",
            isPresented: $showResetConfirmation,
            titleVisibility: .visible
        ) {
            Button("Reset", role: .destructive) {
                PerfMetrics.shared.reset()
                snapshot = PerfMetrics.shared.snapshot()
                Haptics.success()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Clears all counters and restarts the measurement window.")
        }
    }

    // MARK: - Capture toggle

    private var captureSection: some View {
        Section {
            Toggle(isOn: enabledBinding) {
                Label("Capture metrics", systemImage: "gauge.with.dots.needle.67percent")
            }
        } header: {
            Text("Capture")
        } footer: {
            Text("Runs a main-thread stall watchdog and times the kernel/FFI "
                + "hot paths. Adds a small always-on cost, so leave it off when "
                + "not investigating.")
        }
    }

    private var enabledBinding: Binding<Bool> {
        Binding(
            get: { enabled },
            set: { newValue in
                enabled = newValue
                PerfMetrics.shared.setEnabled(newValue)
                Haptics.selection()
            }
        )
    }

    // MARK: - Watchdog

    private var watchdogSection: some View {
        let w = snapshot.watchdog
        return Section {
            statRow("Window", PerfFormat.duration(snapshot.elapsed))
            statRow("Samples", "\(w.sampleCount)")
            statRow("Jank (≥\(Int(PerfMetrics.jankThresholdMillis))ms)", "\(w.jankCount)",
                    tint: w.jankCount > 0 ? AppTheme.Tint.warning : nil)
            statRow("Hangs (≥\(Int(PerfMetrics.hangThresholdMillis))ms)", "\(w.hangCount)",
                    tint: w.hangCount > 0 ? AppTheme.Tint.error : nil)
            statRow("Worst stall", PerfFormat.millis(w.maxStallMillis),
                    tint: w.maxStallMillis >= PerfMetrics.hangThresholdMillis
                        ? AppTheme.Tint.error
                        : (w.maxStallMillis >= PerfMetrics.jankThresholdMillis
                            ? AppTheme.Tint.warning : nil))
            statRow("Now", PerfFormat.millis(w.lastStallMillis))
        } header: {
            Text("Main-thread health")
        } footer: {
            Text("The watchdog pings the main queue ~20×/s and measures how long "
                + "it waits to be serviced. A high \"worst stall\" or any hangs "
                + "mean something is blocking the UI.")
        }
    }

    // MARK: - Operations

    private var operationsSection: some View {
        Section {
            ForEach(PerfOp.allCases, id: \.self) { op in
                operationRow(op, stat: snapshot.ops[op] ?? PerfOpStat())
            }
        } header: {
            Text("Operations")
        } footer: {
            Text("Per-operation cost since the window started. ⚙︎ marks "
                + "main-thread work — that's what blocks the UI. Push-frame "
                + "decode and snapshot pull also report payload size.")
        }
    }

    private func operationRow(_ op: PerfOp, stat: PerfOpStat) -> some View {
        let rate = snapshot.elapsed > 0 ? Double(stat.count) / snapshot.elapsed : 0
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                if op.isMainThread {
                    Image(systemName: "gearshape.fill")
                        .font(.caption2)
                        .foregroundStyle(AppTheme.Tint.warning)
                }
                Text(op.title).font(.subheadline.weight(.medium))
                Spacer()
                Text("\(stat.count)×")
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            HStack(spacing: 12) {
                metric("avg", PerfFormat.micros(stat.avgMicros))
                metric("max", PerfFormat.micros(stat.maxMicros),
                       tint: stat.maxMicros >= 50_000 ? AppTheme.Tint.error
                           : (stat.maxMicros >= 16_000 ? AppTheme.Tint.warning : nil))
                metric("rate", PerfFormat.rate(rate))
                if op.tracksBytes {
                    metric("data", PerfFormat.bytes(stat.totalBytes))
                }
            }
        }
        .padding(.vertical, 2)
    }

    private func metric(_ label: String, _ value: String, tint: Color? = nil) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label.uppercased())
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.monospacedDigit())
                .foregroundStyle(tint ?? .secondary)
        }
    }

    // MARK: - Reset

    private var resetSection: some View {
        Section {
            Button(role: .destructive) {
                showResetConfirmation = true
            } label: {
                Label("Reset metrics", systemImage: "arrow.counterclockwise")
            }
        }
    }

    // MARK: - Row helper

    private func statRow(_ label: String, _ value: String, tint: Color? = nil) -> some View {
        HStack {
            Text(label)
            Spacer()
            Text(value)
                .font(.body.monospacedDigit())
                .foregroundStyle(tint ?? .secondary)
        }
    }
}

// MARK: - Formatting

/// Small value formatters for the Performance HUD. Kept local so the view stays
/// presentation-only and the units are consistent across every row.
private enum PerfFormat {
    /// Microseconds → adaptive `µs` / `ms` string.
    static func micros(_ us: Int) -> String {
        if us < 1_000 { return "\(us)µs" }
        return String(format: "%.1fms", Double(us) / 1_000)
    }

    /// Milliseconds (Double) → `ms`, one decimal under 100ms.
    static func millis(_ ms: Double) -> String {
        if ms < 100 { return String(format: "%.1fms", ms) }
        return String(format: "%.0fms", ms)
    }

    /// Per-second rate.
    static func rate(_ perSec: Double) -> String {
        if perSec == 0 { return "0/s" }
        if perSec < 10 { return String(format: "%.1f/s", perSec) }
        return String(format: "%.0f/s", perSec)
    }

    /// Total byte count → KB/MB.
    static func bytes(_ count: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(count), countStyle: .memory)
    }

    /// Elapsed seconds → `Ns` / `Nm Ns`.
    static func duration(_ seconds: TimeInterval) -> String {
        let s = Int(seconds)
        if s < 60 { return "\(s)s" }
        return "\(s / 60)m \(s % 60)s"
    }
}
