import Foundation

/// Builds the system prompt injected at position 0 of every agent run.
///
/// Surfaces a compact podcast inventory (subscriptions, in-progress
/// episodes, recent unplayed) so the agent can answer "what shows am I
/// subscribed to" or "what was I listening to" without spending a tool call.
/// Detailed drill-downs (transcripts, wiki, semantic search) still go
/// through tools.
///
/// Includes the friend list, recent notes, and persisted memories the
/// template ships with.
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
        Be concise and action-oriented. For specifics that aren't in this prompt
        (transcripts, episode contents, semantic search), call your tools.

        You can play episodes the user is NOT subscribed to. When asked to play
        a guest appearance, a one-off episode, or anything not in the library:
        1. Use `search_podcast_directory` to find the feed URL + audio URL.
        2. Use `play_episode(audio_url:, title:, feed_url:)` to start playing immediately.
           ALWAYS pass feed_url when you have one — the app fetches the show's
           real artwork and title from it. Only omit feed_url for raw audio
           links where you genuinely don't know the source podcast.
        3. Optionally offer `subscribe_podcast(feed_url)` so the user can follow the show.
        For transcripts of external episodes, call `subscribe_podcast` first then
        `download_and_transcribe(feed_url, audio_url)`.

        To browse an unfamiliar show's episodes BEFORE subscribing, call
        `list_episodes` and pass either the `collection_id` (as `podcast_id`)
        or the `feed_url` you got from `search_podcast_directory`. The app
        captures the show's metadata + episodes without flipping the follow
        bit. Only call `subscribe_podcast` when the user explicitly says they
        want to follow the show.

        You are running on a fast/cheap model by default. Before answering, judge
        the request: simple lookups, one-tool answers, short factual replies →
        just answer. If the task needs multi-step reasoning, planning, writing
        code, careful synthesis, or you're not confident you can answer well →
        call `upgrade_thinking` first (no arguments needed; a one-line reason
        helps). Subsequent turns will run on a stronger model. Default to NOT
        upgrading — only upgrade when you're genuinely unsure or the task is
        clearly complex.
        """)

        sections.append(Self.skillsCatalog())

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

        // AI Inbox: archived episodes are silently soft-hidden from the agent's prompt context.
        let inProgress = state.episodes
            .filter { !$0.played && !$0.isTriageArchived && $0.playbackPosition > 0 }
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
        // AI Inbox: archived episodes are silently soft-hidden from the agent's prompt context.
        let recentUnplayed = state.episodes
            .filter { !$0.played && !$0.isTriageArchived && $0.playbackPosition == 0 && $0.pubDate >= cutoff }
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

        if !state.friends.isEmpty {
            let list = state.friends
                .map { "- \($0.displayName) (\(String($0.identifier.prefix(6))))" }
                .joined(separator: "\n")
            sections.append("## Friends\n\(list)")
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
            // consolidated voice the `AgentMemoryCompiler` carries forward.
            sections.append("## What You Know About the User\n\(compiled.text)")
        } else if !memories.isEmpty {
            let list = memories.map { "- \($0.content)" }.joined(separator: "\n")
            sections.append("## What You Know About the User\n\(list)")
        }

        return sections.joined(separator: "\n\n")
    }

    /// Renders the `## Skills` section enumerating every registered skill.
    /// The agent reads this list and calls `use_skill(skill_id:)` to opt in
    /// to a skill's instructions and tools.
    @MainActor
    private static func skillsCatalog() -> String {
        let lines = AgentSkillRegistry.all
            .map { "- `\($0.id)` — \($0.summary)" }
            .joined(separator: "\n")
        return """
        ## Skills
        \(lines)

        Call `use_skill(skill_id: "<id>")` to load any of these — you'll get its full instructions back and unlock its tools for the rest of the conversation. Default to NOT loading a skill unless the user's request matches one. Skill manuals are large; loading a skill you don't need wastes context.
        """
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
