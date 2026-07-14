import Charts
import SwiftUI

struct UsageCostSettingsView: View {
    @ObservedObject private var ledger = CostLedger.shared
    @State private var range: CostRange = .last7Days
    @State private var confirmClear = false

    var body: some View {
        Group {
            if ledger.records.isEmpty {
                empty
            } else {
                scroll
            }
        }
        .navigationTitle("Usage & Cost")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if !ledger.records.isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button(role: .destructive) {
                            confirmClear = true
                        } label: {
                            Label("Clear log", systemImage: "trash")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
        }
        // `.alert` rather than `.confirmationDialog` — iOS 26's
        // popover-promotion can elide the Cancel button. The tap target
        // for this confirm is a red trash glyph in the toolbar; same
        // trap as the other destructive confirms across the app.
        .alert(
            "Clear usage log?",
            isPresented: $confirmClear
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Clear", role: .destructive) { ledger.clear() }
        } message: {
            Text("Removes all usage history. Your API bill won't change.")
        }
    }

    private var empty: some View {
        ContentUnavailableView {
            Label("No usage yet", systemImage: "dollarsign.circle")
        } description: {
            Text("Cost and token counts will appear here after the next AI call.")
        }
    }

    private var scroll: some View {
        let filtered = CostAggregator.filter(ledger.records, by: range)
        return ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                rangePicker
                heroStats(for: filtered)
                dailyChart(for: filtered)
                breakdownSection(
                    title: "By feature",
                    buckets: CostAggregator.aggregate(filtered, by: { CostFeature.displayName(for: $0.feature) })
                )
                breakdownSection(
                    title: "By model",
                    buckets: CostAggregator.aggregate(filtered, by: \.model)
                )
                recentCalls(for: filtered)
                allTimeFooter
                Color.clear.frame(height: 24)
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
        }
        .background(Color(.systemBackground))
    }

    private var rangePicker: some View {
        LiquidGlassSegmentedPicker(
            "Range",
            selection: $range,
            segments: CostRange.allCases.map { ($0, $0.shortLabel) }
        )
    }

    // MARK: Hero stats

    private func heroStats(for records: [UsageRecord]) -> some View {
        let totalCost = records.reduce(0) { $0 + $1.costUSD }
        let totalCalls = records.count
        let avg = totalCalls == 0 ? 0 : totalCost / Double(totalCalls)
        let avgLatency = totalCalls == 0 ? 0 : records.reduce(0) { $0 + $1.latencyMs } / totalCalls

        return VStack(alignment: .leading, spacing: 12) {
            Text(range.displayLabel)
                .font(.caption.weight(.semibold))
                .tracking(1.2)
                .textCase(.uppercase)
                .foregroundStyle(.secondary)

            Text(CostFormatter.usd(totalCost))
                .font(.system(size: 46, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(.primary)

            HStack(spacing: 18) {
                heroMetric(value: "\(totalCalls)", label: "calls")
                Divider().frame(height: 28)
                heroMetric(value: CostFormatter.usdCompact(avg), label: "avg / call")
                Divider().frame(height: 28)
                heroMetric(value: CostFormatter.latency(avgLatency), label: "avg latency")
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    private func heroMetric(value: String, label: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value).font(.headline.monospacedDigit()).foregroundStyle(.primary)
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
    }

    // MARK: Daily chart

    @ViewBuilder
    private func dailyChart(for records: [UsageRecord]) -> some View {
        let series = CostAggregator.dailySeries(for: records)
        if series.isEmpty {
            emptyCard(text: "No spend in this range.")
        } else {
            VStack(alignment: .leading, spacing: 10) {
                sectionLabel("Daily spend")
                Chart(series) { point in
                    BarMark(
                        x: .value("Day", point.day, unit: .day),
                        y: .value("Cost", point.cost)
                    )
                    .foregroundStyle(by: .value("Feature", CostFeature.displayName(for: point.feature)))
                    .cornerRadius(2)
                }
                .chartXAxis {
                    AxisMarks(values: .stride(by: xAxisStride)) { value in
                        AxisGridLine()
                        AxisValueLabel(format: xAxisFormat)
                    }
                }
                .chartYAxis {
                    AxisMarks(position: .leading) { value in
                        AxisGridLine()
                        AxisValueLabel {
                            if let d = value.as(Double.self) {
                                Text(CostFormatter.usdAxis(d))
                            }
                        }
                    }
                }
                .chartLegend(position: .bottom, alignment: .leading, spacing: 8)
                .frame(height: 180)
            }
            .padding(16)
            .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
        }
    }

    // MARK: Breakdown

    private func breakdownSection(title: String, buckets: [CostBucket]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            sectionLabel(title)
            if buckets.isEmpty {
                Text("No data in this range.").font(.subheadline).foregroundStyle(.tertiary)
            } else {
                let maxCost = max(buckets.map(\.cost).max() ?? 0, 0.0001)
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(buckets) { bucket in
                        UsageBucketRow(bucket: bucket, maxCost: maxCost)
                    }
                }
            }
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    // MARK: Recent calls

    private func recentCalls(for records: [UsageRecord]) -> some View {
        let recent = Array(records.prefix(50))
        return VStack(alignment: .leading, spacing: 12) {
            sectionLabel("Recent calls")
            if recent.isEmpty {
                Text("No calls in this range.").font(.subheadline).foregroundStyle(.tertiary)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(recent.enumerated()), id: \.element.id) { index, record in
                        NavigationLink(destination: LLMPayloadDetailView(record: record)) {
                            UsageRecentRow(record: record)
                        }
                        .buttonStyle(.plain)
                        if index != recent.count - 1 { Divider() }
                    }
                }
            }
            if records.count > recent.count {
                Text("Showing 50 of \(records.count) in this range.")
                    .font(.caption2).foregroundStyle(.tertiary)
            }
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    // MARK: Footer

    private var allTimeFooter: some View {
        let all = ledger.records
        let total = all.reduce(0) { $0 + $1.costUSD }
        let first = all.last?.at
        return HStack(spacing: 8) {
            Image(systemName: "clock.arrow.circlepath").foregroundStyle(.secondary)
            Text("Lifetime \(CostFormatter.usd(total)) across \(all.count) calls").foregroundStyle(.secondary)
            if let first {
                Text("·")
                Text("since \(first.formatted(date: .abbreviated, time: .omitted))").foregroundStyle(.secondary)
            }
            Spacer()
        }
        .font(.caption)
        .padding(.vertical, 4)
    }

    // MARK: Helpers

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.caption.weight(.semibold))
            .tracking(1.2)
            .textCase(.uppercase)
            .foregroundStyle(.secondary)
    }

    private func emptyCard(text: String) -> some View {
        Text(text)
            .font(.subheadline).foregroundStyle(.tertiary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    private var xAxisStride: Calendar.Component {
        switch range {
        case .today: return .hour
        case .last7Days, .last30Days: return .day
        case .all: return .weekOfYear
        }
    }

    private var xAxisFormat: Date.FormatStyle {
        switch range {
        case .today: return .dateTime.hour()
        case .last7Days, .last30Days, .all: return .dateTime.month(.abbreviated).day()
        }
    }
}
