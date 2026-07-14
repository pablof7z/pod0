import SwiftUI
import UIKit

struct AgentRunDetailView: View {
    let run: AgentRun

    @Environment(AppStateStore.self) private var store

    @State private var copyFlash: String?
    @State private var shareItem: AgentRunShareItem?

    /// Resolves common podcast-domain argument keys to human-readable
    /// strings so the run detail row reads "play_episode — episode_id:
    /// 'How to Think About Keto', start_seconds: 7:00" instead of
    /// "episode_id: '0123…', start_seconds: 420". Falls through to the
    /// formatter's generic scalar render for unrecognized keys / values.
    private var podcastValueResolver: AgentRunToolFormatter.ValueResolver {
        { [store] key, value in
            switch key {
            case "episode_id":
                if case .string(let s) = value, let id = UUID(uuidString: s),
                   let title = store.episode(id: id)?.title, !title.isEmpty {
                    return "\u{201C}\(title)\u{201D}"
                }
            case "podcast_id", "subscription_id":
                if case .string(let s) = value, let id = UUID(uuidString: s),
                   let title = store.podcast(id: id)?.title, !title.isEmpty {
                    return "\u{201C}\(title)\u{201D}"
                }
            case "timestamp", "start_seconds", "end_seconds", "playhead":
                if let seconds = numericSeconds(from: value) {
                    return Self.formatSeconds(seconds)
                }
            default:
                break
            }
            return nil
        }
    }

    /// Coerces an `AnyCodable` numeric to seconds. Accepts both `Int`
    /// (the common JSON case for whole-second timestamps) and `Double`
    /// (fractional seconds from chunk start times).
    private func numericSeconds(from value: AnyCodable) -> Double? {
        switch value {
        case .int(let i): return Double(i)
        case .double(let d): return d
        default: return nil
        }
    }

    private static func formatSeconds(_ s: Double) -> String {
        let total = max(0, Int(s.rounded()))
        let h = total / 3600
        let m = (total % 3600) / 60
        let sec = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, sec)
            : String(format: "%d:%02d", m, sec)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                headerSection
                if let reason = run.failureReason {
                    failureSection(reason)
                }
                toolsUsedSection
                inspectionLinksSection
                ForEach(run.turns) { turn in
                    AgentRunTurnSection(turn: turn)
                }
                Color.clear.frame(height: 24)
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
        }
        .background(Color(.systemBackground))
        .navigationTitle("Run Details")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbarMenu }
        .overlay(alignment: .top) {
            if let copyFlash {
                Text(copyFlash)
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(.ultraThinMaterial, in: Capsule())
                    .padding(.top, 8)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .sheet(item: $shareItem) { item in
            AgentRunShareSheet(items: [item.text])
        }
    }

    @ToolbarContentBuilder
    private var toolbarMenu: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Menu {
                Button {
                    copyAll()
                } label: {
                    Label("Copy full JSON", systemImage: "doc.on.doc")
                }
                Button {
                    copy(run.systemPrompt, label: "Copied system prompt")
                } label: {
                    Label("Copy system prompt", systemImage: "text.alignleft")
                }
                if !run.initialInput.isEmpty {
                    Button {
                        copy(run.initialInput, label: "Copied initial input")
                    } label: {
                        Label("Copy initial input", systemImage: "text.bubble")
                    }
                }
                if let reason = run.failureReason {
                    Button {
                        copy(reason, label: "Copied failure reason")
                    } label: {
                        Label("Copy failure reason", systemImage: "exclamationmark.triangle")
                    }
                }
                Divider()
                Button {
                    shareItem = AgentRunShareItem(text: exportText())
                } label: {
                    Label("Share…", systemImage: "square.and.arrow.up")
                }
            } label: {
                Image(systemName: "square.and.arrow.up")
            }
            .accessibilityLabel("More actions")
        }
    }

    private var headerSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: AgentRunSourceStyle.icon(run.source))
                    .foregroundStyle(.secondary)
                Text(AgentRunSourceStyle.label(run.source))
                    .font(.headline)
                    .foregroundStyle(.primary)
                Spacer()
                AgentRunOutcomeBadge(outcome: run.finalOutcome, font: .subheadline)
            }

            VStack(alignment: .leading, spacing: 8) {
                detail("Started", run.timestamp.formatted(date: .abbreviated, time: .standard))
                detail("Duration", AgentRunFormat.duration(run.durationMs))
                detail("Turns", "\(run.turns.count)")
                detail("Total tokens", run.totalTokensUsed.formatted())
                detail("ID", run.id.uuidString.lowercased())
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    private func failureSection(_ reason: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Failure", systemImage: "xmark.circle.fill")
                .font(.caption.weight(.semibold))
                .foregroundStyle(AppTheme.Tint.error)
            Text(reason)
                .font(.caption.monospaced())
                .foregroundStyle(.primary)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(AppTheme.Tint.error.opacity(0.12))
        )
    }

    private struct FlatToolEntry: Identifiable {
        let id = UUID()
        let turnNumber: Int
        let dispatch: AgentToolDispatch
    }

    private var flatTools: [FlatToolEntry] {
        run.turns.flatMap { turn in
            turn.toolDispatches.map { FlatToolEntry(turnNumber: turn.turnNumber, dispatch: $0) }
        }
    }

    @ViewBuilder
    private var toolsUsedSection: some View {
        let tools = flatTools
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Tools used")
                    .font(.caption.weight(.semibold))
                    .tracking(1.2)
                    .textCase(.uppercase)
                    .foregroundStyle(.secondary)
                Spacer()
                Text("\(tools.count)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }

            if tools.isEmpty {
                Text("No tools were called in this run.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(tools) { entry in
                        toolRow(entry)
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    private func toolRow(_ entry: FlatToolEntry) -> some View {
        let formatted = AgentRunToolFormatter.format(
            toolName: entry.dispatch.toolName,
            arguments: entry.dispatch.arguments,
            resolveValue: podcastValueResolver
        )
        let isError = entry.dispatch.error != nil
        return HStack(alignment: .top, spacing: 10) {
            Image(systemName: isError ? "xmark.circle.fill" : "checkmark.circle.fill")
                .font(.callout)
                .foregroundStyle(isError ? AppTheme.Tint.error : AppTheme.Tint.success)
                .padding(.top, 2)

            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(formatted.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                    Text(entry.dispatch.toolName)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.tertiary)
                    Spacer()
                    Text("turn \(entry.turnNumber)")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                if let detail = formatted.detail, !detail.isEmpty {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let error = entry.dispatch.error {
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(AppTheme.Tint.error)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.tertiarySystemBackground))
        .cornerRadius(8)
    }

    private var inspectionLinksSection: some View {
        VStack(spacing: 0) {
            NavigationLink {
                AgentRunSystemPromptView(systemPrompt: run.systemPrompt)
            } label: {
                inspectionRow(
                    icon: "text.alignleft",
                    title: "System Prompt",
                    subtitle: "\(run.systemPrompt.count) characters"
                )
            }
            .buttonStyle(.plain)

            Divider()
                .padding(.leading, 44)

            NavigationLink {
                AgentRunMessagesView(turns: run.turns)
            } label: {
                inspectionRow(
                    icon: "ellipsis.bubble",
                    title: "Messages",
                    subtitle: "\(totalMessageCount) messages across \(run.turns.count) turns"
                )
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, 4)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    private func inspectionRow(icon: String, title: String, subtitle: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "chevron.right")
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }

    private var totalMessageCount: Int {
        run.turns.reduce(0) { $0 + $1.messagesBeforeCall.count }
    }

    private func detail(_ label: String, _ value: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(label)
                .lineLimit(1)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }

    private func copyAll() {
        copy(exportText(), label: "Copied full JSON")
    }

    private func copy(_ text: String, label: String) {
        UIPasteboard.general.string = text
        showFlash(label)
    }

    private func showFlash(_ message: String) {
        withAnimation(.easeOut(duration: 0.18)) { copyFlash = message }
        Task {
            try? await Task.sleep(nanoseconds: 1_400_000_000)
            await MainActor.run {
                withAnimation(.easeIn(duration: 0.25)) { copyFlash = nil }
            }
        }
    }

    private func exportText() -> String {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        if let data = try? encoder.encode(run),
           let str = String(data: data, encoding: .utf8) {
            return str
        }
        return String(describing: run)
    }
}
