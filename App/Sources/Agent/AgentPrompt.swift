import Foundation

/// Builds the system prompt injected at position 0 of every agent run.
///
/// Surfaces a compact podcast inventory (subscriptions, in-progress episodes,
/// recent unplayed) and describes only the bounded tools the shared Rust turn
/// currently authorizes.
///
/// Includes recent notes and persisted memories the template ships with.
enum AgentPrompt {

    // Inventory caps — keep the prompt under a few KB even with a heavy library.
    private enum Cap {
        static let subscriptions = 30
        static let inProgress = 5
        static let recentUnplayed = 10
        static let recentWindowDays: Double = 7
        static let titleChars = 80
    }

    @MainActor
    static func build(for state: AppState) -> String {
        var sections: [String] = []

        sections.append("""
        You are a helpful personal assistant embedded in a podcast-player iOS app.
        Today is \(Self.dateString).
        Help the user surface, recall, and reason about what they've been listening to.
        Be concise, action-oriented, and honest about available evidence.

        Available tools can:
        - save a note;
        - list subscriptions, all known podcasts, one podcast's episodes,
          in-progress episodes, or recent unplayed episodes;
        - search episode titles and descriptions in the user's library;
        - pause playback or change playback speed.

        Use only tools exposed with the current request. `list_episodes` requires
        a podcast_id returned by a library tool. Episode search does not inspect
        transcript text. Never invent a quote, timestamp, transcript claim, or
        completed action. When exact transcript evidence is unavailable, say so.
        """)

        // Prompt the agent with the user's followed podcasts only. Synthetic
        // shows (Agent Generated, Unknown) don't carry user follow rows, so
        // they're filtered out by the join.
        let followedPodcastIDs = Set(state.subscriptions.map(\.podcastID))
        let followedPodcasts = state.podcasts.filter { followedPodcastIDs.contains($0.id) }
        if !followedPodcasts.isEmpty {
            let titles = followedPodcasts
                .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
                .prefix(Cap.subscriptions)
                .map { "- \(truncate($0.title))" }
                .joined(separator: "\n")
            let suffix = followedPodcasts.count > Cap.subscriptions
                ? "\n…and \(followedPodcasts.count - Cap.subscriptions) more"
                : ""
            sections.append("## Subscriptions (\(followedPodcasts.count))\n\(titles)\(suffix)")
        }

        let inProgress = state.episodes
            .filter { !$0.played && $0.playbackPosition > 0 }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(Cap.inProgress)
        if !inProgress.isEmpty {
            let lookup = subscriptionTitlesByID(state)
            let lines = inProgress.map { ep -> String in
                let show = lookup[ep.podcastID] ?? "Unknown show"
                return "- \(truncate(ep.title)) — \(show)"
            }.joined(separator: "\n")
            sections.append("## In Progress\n\(lines)")
        }

        let cutoff = Date().addingTimeInterval(-Cap.recentWindowDays * 86_400)
        let recentUnplayed = state.episodes
            .filter { !$0.played && $0.playbackPosition == 0 && $0.pubDate >= cutoff }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(Cap.recentUnplayed)
        if !recentUnplayed.isEmpty {
            let lookup = subscriptionTitlesByID(state)
            let lines = recentUnplayed.map { ep -> String in
                let show = lookup[ep.podcastID] ?? "Unknown show"
                return "- \(truncate(ep.title)) — \(show)"
            }.joined(separator: "\n")
            sections.append("## Recent (last \(Int(Cap.recentWindowDays)) days, unplayed)\n\(lines)")
        }

        let activeNotes = state.notes
            .filter { !$0.deleted && $0.kind != .systemEvent }
            .sorted { $0.createdAt > $1.createdAt }
            .prefix(20)
        if !activeNotes.isEmpty {
            let list = activeNotes.map { "- \($0.text)" }.joined(separator: "\n")
            sections.append("## Notes\n\(list)")
        }

        let memories = state.agentMemories.filter { !$0.deleted }
        if let compiled = state.compiledMemory,
           !compiled.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
           !memories.isEmpty {
            // Prefer the compiled paragraph when available — it's the
            // Prefer the preserved compiled paragraph when it matches active memories.
            sections.append("## What You Know About the User\n\(compiled.text)")
        } else if !memories.isEmpty {
            let list = memories.map { "- \($0.content)" }.joined(separator: "\n")
            sections.append("## What You Know About the User\n\(list)")
        }

        return sections.joined(separator: "\n\n")
    }

    private static func subscriptionTitlesByID(_ state: AppState) -> [UUID: String] {
        Dictionary(uniqueKeysWithValues: state.podcasts.map { ($0.id, $0.title) })
    }

    private static func truncate(_ s: String) -> String {
        s.count <= Cap.titleChars ? s : String(s.prefix(Cap.titleChars - 1)) + "…"
    }

    /// Cached formatter — DateFormatter is expensive to allocate and thread-safe for read after setup.
    private static let dateFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .full
        f.timeStyle = .short
        return f
    }()

    private static var dateString: String {
        dateFormatter.string(from: Date())
    }
}
