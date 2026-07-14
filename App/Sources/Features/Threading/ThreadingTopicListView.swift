import SwiftUI

// MARK: - Threading topic list

/// Index view for every cross-episode topic the inference service has
/// surfaced in the user's library. Lives behind the wiki surface (UX-09 §3
/// explicitly forbids a tab) — opened from the wiki home's "Threads" row.
///
/// Each row shows: editorial topic name, mention counter, contradiction
/// dot, and the relative date of the latest mention. Tap pushes
/// `ThreadingTopicView` for the full timeline.
struct ThreadingTopicListView: View {

    @Environment(AppStateStore.self) private var store
    @State private var service = ThreadingInferenceService.shared
    @State private var hasSeeded = false

    var body: some View {
        content
            .navigationTitle("Threads")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
            .task {
                service.attach(store: store)
#if DEBUG
                if !hasSeeded {
                    hasSeeded = true
                    service.seedMockIfEmpty(store: store)
                }
#endif
            }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        if store.threadingTopics.isEmpty {
            emptyView
        } else {
            list
        }
    }

    private var list: some View {
        List {
            if let lastRecomputedAt = service.lastRecomputedAt {
                Section {
                    Text("Recomputed \(lastRecomputedAt, format: .relative(presentation: .named))")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                .listRowBackground(Color.clear)
            }
            Section {
                ForEach(store.threadingTopics) { topic in
                    NavigationLink {
                        ThreadingTopicView(topicID: topic.id)
                    } label: {
                        TopicRow(topic: topic)
                    }
                    .listRowBackground(Color.clear)
                }
            } header: {
                Text("\(store.threadingTopics.count) topic\(store.threadingTopics.count == 1 ? "" : "s")")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .scrollContentBackground(.hidden)
        .background(paperBackground)
    }

    private var emptyView: some View {
        VStack(spacing: 18) {
            Spacer()
            Image(systemName: "point.3.connected.trianglepath.dotted")
                .font(.system(size: 56, weight: .ultraLight))
                .foregroundStyle(.tertiary)
            Text("No threads yet.")
                .font(.title3)
                .multilineTextAlignment(.center)
            Text("Threads appear once a topic recurs across at least three episodes in your library.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(paperBackground)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Task { await service.recompute(store: store) }
            } label: {
                if service.isRecomputing {
                    ProgressView()
                } else {
                    Image(systemName: "arrow.clockwise")
                }
            }
            .accessibilityLabel("Recompute threads")
            .disabled(service.isRecomputing)
        }
    }

    private var paperBackground: some View {
        Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? UIColor(red: 0.055, green: 0.059, blue: 0.071, alpha: 1)
                : UIColor(red: 0.965, green: 0.949, blue: 0.914, alpha: 1)
        })
        .ignoresSafeArea()
    }
}

// MARK: - Row

/// Single row in the topic list. Editorial serif name, monospaced count for
/// tabular alignment, an amber contradiction dot when applicable.
private struct TopicRow: View {

    let topic: ThreadingTopic

    private static let relative: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(topic.displayName)
                        .font(AppTheme.Typography.body.weight(.semibold))
                        .foregroundStyle(.primary)
                    if topic.contradictionCount > 0 {
                        Circle()
                            .fill(AppTheme.Tint.threadingContradiction)
                            .frame(width: 6, height: 6)
                            .accessibilityHidden(true)
                    }
                }
                if let definition = topic.definition, !definition.isEmpty {
                    Text(definition)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                HStack(spacing: 8) {
                    Label("\(topic.episodeMentionCount)", systemImage: "waveform")
                        .labelStyle(.titleAndIcon)
                    if topic.contradictionCount > 0 {
                        Text("\u{00B7}")
                        Text("\(topic.contradictionCount) contradiction\(topic.contradictionCount == 1 ? "" : "s")")
                    }
                    if let last = topic.lastMentionedAt {
                        Text("\u{00B7}")
                        Text(TopicRow.relative.localizedString(for: last, relativeTo: Date()))
                    }
                }
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    /// VoiceOver assembly. The previous label hard-coded plural-s
    /// ("1 mentions, 1 contradictions") and chattily announced
    /// "0 contradictions" for topics that didn't have any. Now we
    /// plural-match each count and skip contradictions entirely when
    /// there are none, mirroring the visual row.
    private var accessibilityLabel: String {
        var parts: [String] = [topic.displayName]
        let mentions = topic.episodeMentionCount
        parts.append("\(mentions) mention\(mentions == 1 ? "" : "s")")
        if topic.contradictionCount > 0 {
            let n = topic.contradictionCount
            parts.append("\(n) contradiction\(n == 1 ? "" : "s")")
        }
        if let last = topic.lastMentionedAt {
            parts.append("last mentioned \(TopicRow.relative.localizedString(for: last, relativeTo: Date()))")
        }
        return parts.joined(separator: ", ")
    }
}
