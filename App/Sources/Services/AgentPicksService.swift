import Foundation
import Observation
import os.log

// MARK: - AgentPicksService
//
// Curates the "agent picks" surface in the merged Home featured section.
// One LLM call per ~6 hours per category (or on material library change),
// cached in memory; degrades to a deterministic heuristic when no API key
// is set or the call fails.
//
// Magazine mode (May 2026):
//   - Bundles, fingerprints, and refresh tasks are keyed by `UUID?` where
//     `nil` is the All-Categories pseudo-category. Switching from
//     "Learning" to "Entertainment" reads a separate cache slot, so each
//     section feels like flipping the page on a different magazine.
//   - The streaming refresh captures the category id it was started for
//     and writes streamed picks into THAT slot regardless of which
//     category the user is currently viewing — flipping back to the
//     mid-stream category surfaces its (now complete) bundle without a
//     refetch.
//
// Design notes:
//   - We do NOT reuse `AgentChatSession`'s tool loop. The picks need
//     exactly one structured-JSON response from the model — instrumenting
//     the full chat-history + tool-dispatch pipeline would be wasted work
//     and would fight the cache (every send mutates session history).
//   - Cache key combines time + a cheap fingerprint over the inputs the
//     prompt actually depends on, scoped to the active category. Marking
//     a Learning episode played invalidates the Learning slot but leaves
//     the Entertainment slot untouched.
//   - Fallback heuristic ("rarely-opened shows"): the store does not
//     track per-show open counts. As a defensible v1 proxy we pick the
//     three subscriptions whose newest unplayed episode is the LEAST
//     recent — i.e. shows the user appears to have neglected — and take
//     the newest unplayed episode from each.

@MainActor
@Observable
final class AgentPicksService {

    static let shared = AgentPicksService()

    static let logger = Logger.app("AgentPicksService")

    /// Cache TTL. The brief asks for ~6 hours; we honour it as the upper
    /// bound. The fingerprint check below can invalidate sooner.
    static let cacheTTL: TimeInterval = 6 * 3_600

    /// Soft cap on subscriptions enumerated in the prompt. Picks against
    /// a 200-show library otherwise blow past sensible token budgets.
    static let promptSubscriptionCap = 30

    /// Wall-clock idle-stall budget for streaming. If no `onPartialContent`
    /// callback arrives within this window the in-flight request is cancelled
    /// and whatever picks already streamed in are surfaced (or the fallback
    /// fires if nothing landed).
    static let streamStallTimeout: TimeInterval = 20

    /// Cache + active-stream state. See `AgentPicksService+Cache.swift`.
    var bundles: [PicksCategoryKey: HomeAgentPicksBundle] = [:]
    var fingerprints: [PicksCategoryKey: PicksFingerprint] = [:]
    var refreshTasks: [PicksCategoryKey: Task<Void, Never>] = [:]
    var streamingCategory: PicksCategoryKey?
    var streamStalled: Bool = false

    /// Currently-displayed category, set by the view layer at body
    /// composition. Drives the unscoped `bundle` / `isStreaming` accessors
    /// kept around for compatibility with surfaces that don't pipe a
    /// category through (and for the "All" pseudo-category).
    private(set) var activeCategoryKey: PicksCategoryKey = .all

    init() {}

    /// Trigger a refresh for the given category if its cache slot is
    /// stale. Cheap to call from `.task`; coalesces concurrent calls into
    /// a single network turn per category.
    func ensureFreshPicks(
        store: AppStateStore,
        category: PodcastCategory? = nil,
        now: Date = Date()
    ) {
        let key = PicksCategoryKey(categoryID: category?.id)
        activeCategoryKey = key
        let fingerprint = makeFingerprint(store: store, category: category)
        if shouldUseCache(now: now, key: key, fingerprint: fingerprint) {
            return
        }
        guard refreshTasks[key] == nil else { return }
        let task = Task<Void, Never> { [weak self] in
            await self?.refresh(
                store: store,
                category: category,
                key: key,
                fingerprint: fingerprint,
                now: now
            )
        }
        refreshTasks[key] = task
    }

    /// Discard every cached slot so the next `ensureFreshPicks` makes a
    /// fresh call. Used by the manual "Refresh picks" affordance and by
    /// the pull-to-refresh on Home.
    func invalidate() {
        bundles.removeAll()
        fingerprints.removeAll()
    }

    /// Discard a single category's slot. Used when only that section's
    /// inputs changed and a global wipe would penalise other sections.
    func invalidate(categoryID: UUID?) {
        let key = PicksCategoryKey(categoryID: categoryID)
        bundles[key] = nil
        fingerprints[key] = nil
    }

    /// Mark `categoryID` (nil = All) as the surface the view is currently
    /// rendering. Drives `isStreaming` / `bundle` accessors that don't
    /// take an explicit key.
    func setActiveCategory(_ categoryID: UUID?) {
        activeCategoryKey = PicksCategoryKey(categoryID: categoryID)
    }

    // MARK: - Public accessors

    /// Bundle for the currently-active category. Empty when no slot has
    /// been populated yet for this category.
    var bundle: HomeAgentPicksBundle {
        bundles[activeCategoryKey] ?? .empty
    }

    /// `true` while the active category's slot is being streamed in.
    var isStreaming: Bool {
        streamingCategory == activeCategoryKey
    }

    /// `true` while the active category has a network refresh in flight.
    var isRefreshing: Bool {
        refreshTasks[activeCategoryKey] != nil
    }

    /// Bundle for an arbitrary category. Used by the view layer to read
    /// the slot for the category being rendered without mutating the
    /// active key.
    func bundle(for categoryID: UUID?) -> HomeAgentPicksBundle {
        bundles[PicksCategoryKey(categoryID: categoryID)] ?? .empty
    }

    func isStreaming(for categoryID: UUID?) -> Bool {
        streamingCategory == PicksCategoryKey(categoryID: categoryID)
    }

    func isRefreshing(for categoryID: UUID?) -> Bool {
        refreshTasks[PicksCategoryKey(categoryID: categoryID)] != nil
    }

    // MARK: - Refresh

    private func refresh(
        store: AppStateStore,
        category: PodcastCategory?,
        key: PicksCategoryKey,
        fingerprint: PicksFingerprint,
        now: Date
    ) async {
        defer {
            refreshTasks[key] = nil
            if streamingCategory == key {
                streamingCategory = nil
            }
        }

        let inputs = collectInputs(store: store, category: category)
        guard !inputs.unplayed.isEmpty else {
            bundles[key] = .empty
            fingerprints[key] = fingerprint
            return
        }

        if hasAPIKey(model: store.state.settings.agentInitialModel) {
            do {
                let picks = try await runLLMPicks(
                    store: store,
                    inputs: inputs,
                    category: category,
                    key: key,
                    now: now
                )
                if !picks.isEmpty {
                    bundles[key] = HomeAgentPicksBundle(
                        picks: picks,
                        source: .agent,
                        generatedAt: now
                    )
                    fingerprints[key] = fingerprint
                    return
                }
            } catch {
                Self.logger.error("Agent picks LLM call failed: \(error.localizedDescription, privacy: .public)")
            }
        }

        // Fallback heuristic — rarely-opened shows proxied via stalest
        // newest-unplayed. The view surfaces these without the agent
        // rationale styling so the source is honest to the user.
        let fallback = AgentPicksFallback.derive(inputs: inputs)
        bundles[key] = HomeAgentPicksBundle(
            picks: fallback,
            source: .fallback,
            generatedAt: now
        )
        fingerprints[key] = fingerprint
    }

    // MARK: - LLM call

    private func hasAPIKey(model: String) -> Bool {
        let reference = LLMModelReference(storedID: model)
        return LLMProviderCredentialResolver.hasAPIKey(for: reference.provider)
    }

    private func runLLMPicks(
        store: AppStateStore,
        inputs: AgentPicksInputs,
        category: PodcastCategory?,
        key: PicksCategoryKey,
        now: Date
    ) async throws -> [HomeAgentPick] {
        let framing = category.flatMap(AgentPicksPrompt.CategoryFraming.make(from:))
        let prompt = AgentPicksPrompt.build(inputs: inputs, framing: framing)
        let messages: [[String: Any]] = [
            ["role": "system", "content": AgentPicksPrompt.systemInstruction(for: framing)],
            ["role": "user", "content": prompt]
        ]
        let knownIDs = Set(inputs.unplayed.map(\.id) + inputs.inProgress.map(\.id))

        // Incremental parser + last-token timestamp for stall detection.
        let parser = AgentPicksStreamingParser()
        let activity = StreamActivity()
        await activity.bump(to: Date())

        streamingCategory = key
        streamStalled = false
        // Reset the slot for THIS category only — leave other categories'
        // cached bundles intact.
        bundles[key] = HomeAgentPicksBundle(picks: [], source: .agent, generatedAt: now)

        let model = store.state.settings.agentInitialModel

        // Streaming task — does the actual network call and incremental parse.
        let streamingTask = Task<[HomeAgentPick], Error> { @MainActor [weak self] in
            let result = try await AgentLLMClient.streamCompletion(
                messages: messages,
                tools: [],
                model: model,
                feature: CostFeature.agentChat,
                onPartialContent: { [weak self] partial in
                    guard let self else { return }
                    Task { await activity.bump(to: Date()) }
                    let events = parser.feed(partial, knownEpisodeIDs: knownIDs)
                    for event in events {
                        self.appendStreamedPick(event, key: key, now: now)
                    }
                }
            )
            // Safety-net parse: if the model emitted everything in a
            // single non-incremental chunk (some providers do this), the
            // incremental parser may have nothing to do, so re-parse the
            // full string with the tolerant end-of-stream parser.
            let text = (result.assistantMessage["content"] as? String) ?? ""
            let alreadyPicked = self?.bundles[key]?.picks ?? []
            if alreadyPicked.isEmpty {
                return AgentPicksPrompt.parse(text, knownEpisodeIDs: knownIDs)
            }
            return alreadyPicked
        }

        // Stall watchdog — cancels the streaming task if no new content
        // has arrived inside `streamStallTimeout`.
        let watchdog = Task { [weak self] in
            while !Task.isCancelled {
                let last = await activity.lastTokenAt
                if Date().timeIntervalSince(last) >= Self.streamStallTimeout {
                    await MainActor.run { self?.streamStalled = true }
                    streamingTask.cancel()
                    return
                }
                try? await Task.sleep(nanoseconds: 500_000_000)
            }
        }
        defer { watchdog.cancel() }

        do {
            return try await streamingTask.value
        } catch {
            // Stall path: prefer whatever streamed in to nothing.
            let partial = bundles[key]?.picks ?? []
            if streamStalled, !partial.isEmpty {
                Self.logger.notice("Agent picks stream stalled; surfacing \(partial.count, privacy: .public) early picks.")
                return partial
            }
            throw error
        }
    }

    /// Append one streamed pick to the slot for `key`. Dedupes by
    /// `episodeID` — the streaming parser shouldn't emit the same id twice,
    /// but if a model echoes an episode across hero+secondary the first
    /// emission wins.
    private func appendStreamedPick(
        _ event: AgentPicksStreamEvent,
        key: PicksCategoryKey,
        now: Date
    ) {
        let current = bundles[key] ?? HomeAgentPicksBundle(picks: [], source: .agent, generatedAt: now)
        guard !current.picks.contains(where: { $0.episodeID == event.episodeID }) else {
            return
        }
        let pick = HomeAgentPick(
            episodeID: event.episodeID,
            rationale: event.reason,
            spokenRationale: event.spokenReason,
            isHero: event.slot == .hero
        )
        var next = current.picks
        next.append(pick)
        bundles[key] = HomeAgentPicksBundle(picks: next, source: .agent, generatedAt: now)
    }
}

// MARK: - StreamActivity

/// Tiny actor holding the "last token arrived at" timestamp. Lets the
/// streaming task (MainActor) and the watchdog (background) share state
/// without either owning the other.
private actor StreamActivity {
    private(set) var lastTokenAt: Date = .distantPast
    func bump(to date: Date) {
        lastTokenAt = date
    }
}

// MARK: - Inputs to both LLM + fallback

struct AgentPicksInputs: Sendable {
    let unplayed: [Episode]
    let inProgress: [Episode]
    let subscriptionTitles: [UUID: String]
    let memorySnippets: [String]
    let topicNames: [String]
}

// MARK: - Fallback heuristic

enum AgentPicksFallback {

    /// Pick up to 3 episodes from the "rarely opened" shows: rank
    /// subscriptions by their newest unplayed episode's `pubDate`
    /// ascending (stalest first), then take the newest unplayed
    /// episode from each of the top 3. The first becomes the hero.
    ///
    /// The heuristic isn't a true open-count — the store doesn't track
    /// opens — but it surfaces the same UX outcome ("here's a show you
    /// haven't been listening to") without adding a new persistence
    /// surface for v1.
    static func derive(inputs: AgentPicksInputs) -> [HomeAgentPick] {
        guard !inputs.unplayed.isEmpty else { return [] }

        // newest-unplayed per show
        var newestPerShow: [UUID: Episode] = [:]
        for ep in inputs.unplayed {
            if let existing = newestPerShow[ep.podcastID] {
                if ep.pubDate > existing.pubDate {
                    newestPerShow[ep.podcastID] = ep
                }
            } else {
                newestPerShow[ep.podcastID] = ep
            }
        }

        // Stalest first: the show whose freshest unplayed episode is the
        // *oldest* is the show the user has been ignoring the longest.
        let sorted = newestPerShow.values.sorted { $0.pubDate < $1.pubDate }
        let top = Array(sorted.prefix(3))

        return top.enumerated().map { idx, ep in
            let showName = inputs.subscriptionTitles[ep.podcastID] ?? "this show"
            return HomeAgentPick(
                episodeID: ep.id,
                rationale: "From \(showName) — you haven't tuned in for a while.",
                isHero: idx == 0
            )
        }
    }
}
