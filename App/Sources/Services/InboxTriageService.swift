import Foundation
import Observation
import os.log

// MARK: - InboxTriageService
//
// Autonomous AI Inbox triage. Runs after the feed refresh pipeline lands
// new episodes; for each untriaged episode published in the recency
// window the agent decides `inbox` (surface on Home with a one-line
// rationale) or `archived` (silently dismiss — episode stays on the show
// page but drops out of unplayed counts and the recent feed).
//
// Triage is "full autonomy" by design: no review surface is rendered for
// archived episodes. The user trusts the agent and recovers from rare
// misses by visiting the show page.
//
// Implementation choices:
//   • One in-flight task at a time. Caller is `SubscriptionRefreshService`
//     (post-refresh) plus on-demand triggers like pull-to-refresh; both
//     coalesce onto the same task.
//   • Decisions land via `AppStateStore.applyTriageDecisions(_:)` in a
//     single mutation batch — SwiftUI re-renders once per pass, not once
//     per episode.
//   • Falls back silently when there's no API key configured. The Inbox
//     section degrades to the newest-unplayed-per-show heuristic via
//     `heuristicPatches` so the rail isn't empty until credentials arrive.

@MainActor
@Observable
final class InboxTriageService {

    static let shared = InboxTriageService()

    static let logger = Logger.app("InboxTriageService")

    /// How far back to consider an episode "new enough to triage". Older
    /// untriaged episodes (e.g. catalog imports) stay untriaged so they
    /// keep behaving like the pre-Inbox app and don't disappear from the
    /// user's library unexpectedly. The forward-looking pass owns
    /// freshly-arrived episodes only.
    static let recencyWindow: TimeInterval = 14 * 86_400

    /// Soft cap on candidates per pass. Triaging 500 candidates blows
    /// token budget; we sample the freshest `candidateCap` and leave the
    /// rest untriaged for next pass — they're stale anyway and won't
    /// hurt from being skipped this round.
    static let candidateCap = 30

    /// Per-show engagement history depth. The LLM sees finished /
    /// unplayed counts over the last `engagementLookback` episodes per
    /// show — enough to capture a behavioural signal without flooding
    /// the prompt.
    static let engagementLookback = 20

    /// A subscription is "newly subscribed" when the follow happened
    /// inside this window — the user has not had a chance to demonstrate
    /// disengagement yet, so the agent is told NOT to archive its
    /// episodes. Drives the `isNewlySubscribed` flag passed to the LLM.
    static let newlySubscribedWindow: TimeInterval = 7 * 86_400

    /// Fallback: a show with zero total signal (no played + no unplayed
    /// in the lookback window) is also treated as newly subscribed even
    /// if the follow is older — covers orphan podcasts and edge cases
    /// where engagement history evaporated.
    static let minSignalEpisodes = 1

    /// `true` while a triage pass is in flight. Surfaced for the UI so
    /// the Home Inbox section can render a shimmer while decisions
    /// stream in.
    private(set) var isRunning: Bool = false

    /// Timestamp of the last successful pass. Used by the Home Inbox
    /// section to disambiguate "no inbox yet" (just installed; no triage
    /// has run) from "triage ran and the agent surfaced nothing".
    private(set) var lastCompletedAt: Date?

    private var activeTask: Task<Void, Never>?

    init() {}

    /// Kick a triage pass for untriaged recent episodes. Coalesces
    /// concurrent calls onto the in-flight task. Fire-and-forget; the
    /// service writes decisions back through the store and the UI picks
    /// them up via the standard observation flow.
    func triageNewEpisodes(store: AppStateStore) {
        if let existing = activeTask, !existing.isCancelled {
            return
        }
        let task = Task<Void, Never> { [weak self] in
            await self?.run(store: store)
        }
        activeTask = task
    }

    // MARK: - Pass

    private func run(store: AppStateStore) async {
        isRunning = true
        defer {
            isRunning = false
            activeTask = nil
        }

        let candidates = selectCandidates(store: store)
        guard !candidates.isEmpty else {
            Self.logger.debug("Triage skipped: no untriaged recent episodes.")
            return
        }

        let engagement = InboxTriageEngagementBuilder.build(
            store: store,
            podcastIDs: Set(candidates.map { $0.podcastID }),
            showTitles: subscriptionTitles(store: store),
            engagementLookback: Self.engagementLookback,
            newlySubscribedWindow: Self.newlySubscribedWindow,
            minSignalEpisodes: Self.minSignalEpisodes
        )

        // No-key fallback: rather than leave the Home Inbox empty when
        // the user hasn't connected an LLM provider yet, surface the
        // newest unplayed episode from each subscription with a
        // heuristic rationale. Matches the safety-net the previous
        // featured surface offered so installs without credentials
        // still have something on the Inbox rail.
        guard hasAPIKey(model: store.state.settings.agentInitialModel) else {
            let patches = heuristicPatches(from: candidates)
            if !patches.isEmpty {
                store.applyTriageDecisions(patches)
                lastCompletedAt = Date()
                Self.logger.notice(
                    "Triage fallback (no API key) seeded inbox with \(patches.count, privacy: .public) heuristic picks."
                )
            }
            return
        }

        do {
            let patches = try await runLLM(
                store: store,
                candidates: candidates.map { $0.candidate },
                engagement: engagement
            )
            if !patches.isEmpty {
                store.applyTriageDecisions(patches)
                lastCompletedAt = Date()
                let inboxCount = patches.filter { $0.decision == .inbox }.count
                let archivedCount = patches.count - inboxCount
                Self.logger.notice(
                    "Triage applied \(patches.count, privacy: .public) decisions (inbox=\(inboxCount, privacy: .public), archived=\(archivedCount, privacy: .public))."
                )
            }
        } catch {
            Self.logger.error("Triage failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    // MARK: - Heuristic fallback

    /// Deterministic, no-LLM seed for the Inbox when there's no API key.
    /// Pick the newest unplayed candidate per show, cap at 5, and
    /// generate a brief "Newest from <show>" rationale so the card has
    /// a "because" line. Other candidates stay untriaged so the next
    /// pass (once a key arrives) can run the full LLM classifier.
    private func heuristicPatches(from bundles: [CandidateBundle]) -> [AppStateStore.TriagePatch] {
        var newestPerShow: [UUID: CandidateBundle] = [:]
        for bundle in bundles {
            if let existing = newestPerShow[bundle.podcastID] {
                if bundle.candidate.pubDate > existing.candidate.pubDate {
                    newestPerShow[bundle.podcastID] = bundle
                }
            } else {
                newestPerShow[bundle.podcastID] = bundle
            }
        }
        return newestPerShow.values
            .sorted { $0.candidate.pubDate > $1.candidate.pubDate }
            .prefix(5)
            .map { bundle in
                AppStateStore.TriagePatch(
                    episodeID: bundle.candidate.id,
                    decision: .inbox,
                    rationale: "Newest from \(bundle.candidate.showTitle)."
                )
            }
    }

    // MARK: - Candidate selection

    private struct CandidateBundle {
        let candidate: InboxTriageCandidate
        let podcastID: UUID
    }

    private func selectCandidates(store: AppStateStore) -> [CandidateBundle] {
        let cutoff = Date().addingTimeInterval(-Self.recencyWindow)
        let titles = subscriptionTitles(store: store)
        let followed = Set(store.state.subscriptions.map(\.podcastID))

        let pool = store.state.episodes
            .lazy
            .filter { ep in
                ep.isUntriaged &&
                !ep.played &&
                ep.pubDate >= cutoff &&
                followed.contains(ep.podcastID)
            }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(Self.candidateCap)

        return pool.map { ep in
            CandidateBundle(
                candidate: InboxTriageCandidate(
                    id: ep.id,
                    showTitle: titles[ep.podcastID] ?? "Unknown show",
                    title: ep.title,
                    pubDate: ep.pubDate,
                    durationMinutes: ep.duration.map { Int($0 / 60) }
                ),
                podcastID: ep.podcastID
            )
        }
    }

    private func subscriptionTitles(store: AppStateStore) -> [UUID: String] {
        var titles: [UUID: String] = [:]
        let followed = Set(store.state.subscriptions.map(\.podcastID))
        for podcast in store.allPodcasts where followed.contains(podcast.id) {
            titles[podcast.id] = podcast.title
        }
        return titles
    }

    // MARK: - LLM call

    private func hasAPIKey(model: String) -> Bool {
        let reference = LLMModelReference(storedID: model)
        return LLMProviderCredentialResolver.hasAPIKey(for: reference.provider)
    }

    private func runLLM(
        store: AppStateStore,
        candidates: [InboxTriageCandidate],
        engagement: [InboxTriageShowEngagement]
    ) async throws -> [AppStateStore.TriagePatch] {
        let input = InboxTriageInput(candidates: candidates, engagement: engagement)
        let prompt = InboxTriagePrompt.build(input: input)
        let messages: [[String: Any]] = [
            ["role": "system", "content": InboxTriagePrompt.systemInstruction],
            ["role": "user", "content": prompt]
        ]
        let knownIDs = Set(candidates.map { $0.id })
        let model = store.state.settings.agentInitialModel

        let result = try await AgentLLMClient.streamCompletion(
            messages: messages,
            tools: [],
            model: model,
            feature: CostFeature.agentChat,
            onPartialContent: { _ in }
        )
        let text = (result.assistantMessage["content"] as? String) ?? ""
        let parsed = InboxTriagePrompt.parse(text, knownEpisodeIDs: knownIDs)
        var patches: [AppStateStore.TriagePatch] = []
        patches.reserveCapacity(parsed.count)
        for (id, decision) in parsed {
            switch decision {
            case .inbox(let rationale, let isHero):
                // Inbox cards MUST have a "Because …" rationale — that
                // line is what makes the surface feel like an editorial
                // decision instead of an arbitrary pick. If the model
                // returned an empty reason, skip the patch entirely so
                // the episode stays untriaged and gets another shot on
                // the next pass rather than rendering as a chip-less
                // card now.
                let trimmed = rationale.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else {
                    Self.logger.debug("Dropped empty-rationale inbox decision for \(id.uuidString, privacy: .public).")
                    continue
                }
                patches.append(AppStateStore.TriagePatch(
                    episodeID: id,
                    decision: .inbox,
                    rationale: trimmed,
                    isHero: isHero
                ))
            case .archived:
                patches.append(AppStateStore.TriagePatch(
                    episodeID: id,
                    decision: .archived,
                    rationale: nil
                ))
            }
        }
        return patches
    }
}
