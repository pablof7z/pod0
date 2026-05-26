import SwiftUI

struct WhatsNewSheet: View {

    let entries: [WhatsNewEntry]
    @Environment(\.dismiss) private var dismiss

    @AppStorage("whatsNew.lastSeenAt") private var lastSeenAtString: String = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                    Text("SINCE YOU LAST OPENED POD0")
                        .font(AppTheme.Typography.caption2.weight(.semibold))
                        .tracking(0.5)
                        .foregroundStyle(.secondary)
                    ForEach(entries) { entry in
                        entrySection(entry)
                    }
                    gotItButton.padding(.top, AppTheme.Spacing.sm)
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.md)
            }
            .navigationTitle("What's new")
            .navigationBarTitleDisplayMode(.large)
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onDisappear {
            if let newest = entries.first {
                lastSeenAtString = Self.iso8601.string(from: newest.shippedAt)
            }
        }
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
            }
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

    private static func dateline(for date: Date) -> String {
        let cal = Calendar.current
        let comps = cal.dateComponents([.month, .day, .hour, .minute], from: date)
        let month = cal.shortMonthSymbols.indices.contains((comps.month ?? 1) - 1)
            ? cal.shortMonthSymbols[(comps.month ?? 1) - 1].uppercased() : ""
        return String(format: "%@ %d \u{00B7} %02d:%02d", month, comps.day ?? 0, comps.hour ?? 0, comps.minute ?? 0)
    }
}
