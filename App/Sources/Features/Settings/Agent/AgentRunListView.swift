import SwiftUI

struct AgentRunListView: View {
    @ObservedObject private var logger = AgentRunLogger.shared
    @State private var filterStore = AgentRunFilterStore()
    @State private var confirmClear = false

    private var filteredRuns: [AgentRun] {
        let f = filterStore.filter
        if f.isEmpty { return logger.runs }
        return logger.runs.filter { f.matches($0) }
    }

    var body: some View {
        Group {
            if logger.runs.isEmpty {
                empty
            } else {
                scroll
            }
        }
        .navigationTitle("Agent Runs")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if !logger.runs.isEmpty {
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
        .confirmationDialog(
            "Clear agent run log?",
            isPresented: $confirmClear,
            titleVisibility: .visible
        ) {
            Button("Clear", role: .destructive) { logger.clear() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This cannot be undone.")
        }
    }

    private var empty: some View {
        ContentUnavailableView {
            Label("No agent runs yet", systemImage: "brain")
        } description: {
            Text("Agent activity will appear here after the first run.")
        }
    }

    private var noMatchesEmpty: some View {
        VStack(spacing: 8) {
            Image(systemName: "line.3.horizontal.decrease.circle")
                .font(.title)
                .foregroundStyle(.secondary)
            Text("No runs match your filters")
                .font(.subheadline.weight(.medium))
            Button("Clear filters") { filterStore.filter = .empty }
                .font(.caption)
                .buttonStyle(.bordered)
                .controlSize(.small)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 32)
        .foregroundStyle(.secondary)
    }

    private var scroll: some View {
        let runs = filteredRuns
        return ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                AgentRunFilterBar(
                    filter: Binding(
                        get: { filterStore.filter },
                        set: { filterStore.filter = $0 }
                    ),
                    totalRuns: logger.runs.count,
                    filteredRuns: runs.count
                )

                if runs.isEmpty {
                    noMatchesEmpty
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(runs.enumerated()), id: \.element.id) { index, run in
                            NavigationLink(destination: AgentRunDetailView(run: run)) {
                                runRow(run)
                            }
                            if index != runs.count - 1 {
                                Divider()
                            }
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.top, 4)
                }

                Color.clear.frame(height: 24)
            }
        }
        .background(Color(.systemBackground))
    }

    private func runRow(_ run: AgentRun) -> some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Image(systemName: AgentRunSourceStyle.icon(run.source))
                        .foregroundStyle(.secondary)
                    Text(AgentRunSourceStyle.label(run.source))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.primary)
                    Spacer()
                    Text(run.timestamp.formatted(date: .abbreviated, time: .shortened))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Text(run.initialInput)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.tail)

                HStack(spacing: 12) {
                    Label("\(run.turns.count) turn\(run.turns.count == 1 ? "" : "s")", systemImage: "repeat")
                    Label("\(run.totalTokensUsed) token\(run.totalTokensUsed == 1 ? "" : "s")", systemImage: "function")
                    Label(AgentRunFormat.duration(run.durationMs), systemImage: "timer")
                }
                .font(.caption2)
                .foregroundStyle(.tertiary)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 4) {
                AgentRunOutcomeBadge(outcome: run.finalOutcome, font: .caption2)
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 12)
        .foregroundStyle(.primary)
    }
}

enum AgentRunSourceStyle {
    static func icon(_ source: AgentRunSource) -> String {
        switch source {
        case .typedChat:     return "keyboard"
        case .voiceMessage:  return "waveform"
        case .nostrInbound:  return "bolt"
        case .manual:        return "play"
        case .scheduledTask: return "clock"
        }
    }

    static func label(_ source: AgentRunSource) -> String {
        switch source {
        case .typedChat:     return "Typed chat"
        case .voiceMessage:  return "Voice message"
        case .nostrInbound:  return "Nostr inbound"
        case .manual:        return "Manual"
        case .scheduledTask: return "Scheduled task"
        }
    }
}

enum AgentRunOutcomeStyle {
    static func iconName(_ outcome: AgentRunOutcome) -> String {
        switch outcome {
        case .completed: return "checkmark.circle.fill"
        case .turnsExhausted: return "exclamationmark.circle.fill"
        case .failed: return "xmark.circle.fill"
        case .cancelled: return "slash.circle.fill"
        }
    }

    static func color(_ outcome: AgentRunOutcome) -> Color {
        switch outcome {
        case .completed: return .green
        case .turnsExhausted: return .yellow
        case .failed: return .red
        case .cancelled: return .gray
        }
    }
}

struct AgentRunOutcomeBadge: View {
    let outcome: AgentRunOutcome
    let font: Font

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: AgentRunOutcomeStyle.iconName(outcome))
                .font(font)
            Text(outcome.rawValue)
                .font(font.weight(.medium))
        }
        .foregroundStyle(AgentRunOutcomeStyle.color(outcome))
    }
}

enum AgentRunFormat {
    /// Compact wall-clock representation. Past 60 seconds we switch to
    /// `Mm Ss` so a 2-minute run reads as "2m 0s" instead of "120.0s",
    /// which was both ambiguous (long-tail decimal) and harder to scan
    /// alongside other timestamps in the list.
    static func duration(_ ms: Int) -> String {
        if ms < 1000 { return "\(ms)ms" }
        if ms < 60_000 { return String(format: "%.1fs", Double(ms) / 1000) }
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        if minutes < 60 { return "\(minutes)m \(seconds)s" }
        let hours = minutes / 60
        let remainingMinutes = minutes % 60
        return "\(hours)h \(remainingMinutes)m"
    }
}
