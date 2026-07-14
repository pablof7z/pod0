import SwiftUI

// MARK: - WhatsNewSheet
//
// Surfaces the changelog of entries the user hasn't seen since the last
// time they opened the app. Renders one section per entry (each entry
// = one commit shipped to the device). On dismiss, persists the newest
// visible entry's id as the "last seen" marker so the same content
// doesn't re-surface on the next launch.
//
// Wired in `AppMain.swift` — see the `.sheet(isPresented:)` chain on
// `RootView()`. Don't present this sheet from anywhere else; the call
// site owns the "should we show it?" decision.

struct WhatsNewSheet: View {

    let entries: [WhatsNewEntry]
    @Environment(\.dismiss) private var dismiss

    /// Mirrors `WhatsNewService.lastSeenAtKey` so dismissing the sheet
    /// advances the marker via the same UserDefaults key the service
    /// reads on next cold launch.
    @AppStorage("whatsNew.lastSeenAt") private var lastSeenAtString: String = ""

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("What's new")
                .navigationBarTitleDisplayMode(.large)
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onDisappear {
            // Persist the marker on any dismissal path (swipe-down or "Got it"
            // tap). "Got it" writes the same value before calling dismiss(), so
            // this is idempotent in that case.
            if let newest = entries.first {
                lastSeenAtString = Self.iso8601.string(from: newest.shippedAt)
            }
        }
    }

    // MARK: - Content

    private var content: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                subtitle
                ForEach(entries) { entry in
                    entrySection(entry)
                }
                gotItButton
                    .padding(.top, AppTheme.Spacing.sm)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.md)
        }
    }

    private var subtitle: some View {
        Text("SINCE YOU LAST OPENED PODCASTR")
            .font(AppTheme.Typography.caption2.weight(.semibold))
            .tracking(0.5)
            .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private func entrySection(_ entry: WhatsNewEntry) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text(Self.dateline(for: entry.shippedAt))
                .font(AppTheme.Typography.caption2.weight(.semibold))
                .tracking(0.5)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                ForEach(Array(entry.lines.enumerated()), id: \.offset) { _, line in
                    lineRow(line)
                }
            }
        }
    }

    private func lineRow(_ line: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.sm) {
            Image(systemName: "sparkle")
                .font(.body)
                .foregroundStyle(.tint)
                .accessibilityHidden(true)
            Text(line)
                .font(AppTheme.Typography.body)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var gotItButton: some View {
        HStack {
            Spacer()
            Button("Got it") {
                if let newest = entries.first {
                    lastSeenAtString = Self.iso8601.string(from: newest.shippedAt)
                }
                Haptics.success()
                dismiss()
            }
            .buttonStyle(.glassProminent)
            Spacer()
        }
    }

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    // MARK: - Formatting

    /// "MAY 10 · 22:45" — uppercase short month/day and 24h time, joined
    /// with a thin middle dot. Matches the editorial dateline style used
    /// elsewhere (Home dateline, episode shippedAt rows).
    private static func dateline(for date: Date) -> String {
        let cal = Calendar.current
        let comps = cal.dateComponents([.month, .day, .hour, .minute], from: date)
        let monthSymbols = cal.shortMonthSymbols
        let monthIndex = (comps.month ?? 1) - 1
        let month = monthSymbols.indices.contains(monthIndex)
            ? monthSymbols[monthIndex].uppercased()
            : ""
        let day = comps.day ?? 0
        let hour = comps.hour ?? 0
        let minute = comps.minute ?? 0
        return String(format: "%@ %d \u{00B7} %02d:%02d", month, day, hour, minute)
    }
}
