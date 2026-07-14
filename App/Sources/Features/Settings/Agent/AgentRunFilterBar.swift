import SwiftUI

/// Filter strip displayed above the run list. Three rows:
///   1. Source chips (multi-select OR)
///   2. Outcome chips (multi-select OR)
///   3. Tool-name search field (case-insensitive substring match)
/// Categories compose with AND. Empty selection in a category =
/// no constraint from that category.
struct AgentRunFilterBar: View {
    @Binding var filter: AgentRunFilter

    /// Total run count before filtering — shown in the header so the
    /// user can tell at a glance how aggressive the current filter is.
    let totalRuns: Int
    /// Result count after filtering. Same purpose as `totalRuns`.
    let filteredRuns: Int

    private let allSources: [AgentRunSource] = [.typedChat, .voiceMessage, .nostrInbound, .manual]
    private let allOutcomes: [AgentRunOutcome] = [.completed, .turnsExhausted, .failed, .cancelled]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            header
            sourceRow
            outcomeRow
            searchField
        }
        .padding(.horizontal, 20)
        .padding(.top, 10)
        .padding(.bottom, 8)
        .background(Color(.systemBackground))
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(countLabel)
                .font(.caption.weight(.semibold))
                .tracking(1.2)
                .textCase(.uppercase)
                .foregroundStyle(.secondary)
            Spacer()
            if !filter.isEmpty {
                Button {
                    filter = .empty
                } label: {
                    Label("Clear", systemImage: "xmark.circle.fill")
                        .labelStyle(.titleAndIcon)
                        .font(.caption2.weight(.medium))
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
            }
        }
    }

    private var countLabel: String {
        if filter.isEmpty {
            return "\(totalRuns) run\(totalRuns == 1 ? "" : "s")"
        }
        return "\(filteredRuns) of \(totalRuns)"
    }

    private var sourceRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(allSources, id: \.self) { source in
                    chip(
                        label: AgentRunSourceStyle.label(source),
                        icon: AgentRunSourceStyle.icon(source),
                        isOn: filter.sources.contains(source)
                    ) {
                        toggle(source: source)
                    }
                }
            }
        }
    }

    private var outcomeRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(allOutcomes, id: \.self) { outcome in
                    chip(
                        label: outcome.rawValue,
                        icon: AgentRunOutcomeStyle.iconName(outcome),
                        tint: AgentRunOutcomeStyle.color(outcome),
                        isOn: filter.outcomes.contains(outcome)
                    ) {
                        toggle(outcome: outcome)
                    }
                }
            }
        }
    }

    private var searchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "wrench.and.screwdriver")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Filter by tool name", text: $filter.toolNameQuery)
                .font(.subheadline)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled(true)
                .submitLabel(.search)
            if !filter.toolNameQuery.isEmpty {
                Button {
                    filter.toolNameQuery = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.tertiary)
                }
                .buttonStyle(.borderless)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    @ViewBuilder
    private func chip(
        label: String,
        icon: String,
        tint: Color = .accentColor,
        isOn: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.caption2)
                Text(label)
                    .font(.caption.weight(.medium))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .foregroundStyle(isOn ? AnyShapeStyle(.white) : AnyShapeStyle(.primary))
            .background(
                Capsule(style: .continuous)
                    .fill(isOn ? AnyShapeStyle(tint) : AnyShapeStyle(Color(.secondarySystemBackground)))
            )
        }
        .buttonStyle(.plain)
    }

    private func toggle(source: AgentRunSource) {
        if filter.sources.contains(source) {
            filter.sources.remove(source)
        } else {
            filter.sources.insert(source)
        }
    }

    private func toggle(outcome: AgentRunOutcome) {
        if filter.outcomes.contains(outcome) {
            filter.outcomes.remove(outcome)
        } else {
            filter.outcomes.insert(outcome)
        }
    }
}
