import Foundation

// MARK: - Wiki triggers

/// Declarative rules describing *what should happen* when the system
/// state changes — without actually scheduling the work.
///
/// `WikiTriggers` is a pure producer: callers feed it events, it returns
/// a list of `WikiRefreshJob`s. A real scheduler (out of scope for lane 7)
/// is expected to dedupe, prioritise, and run them. Splitting trigger
/// logic from execution makes the rules unit-testable and lets us swap
/// the executor (background task, foreground task, server-side worker)
/// without touching the rules.
enum WikiTriggers {

    // MARK: - Inputs

    /// A change in the source data that may invalidate one or more pages.
    enum Event: Sendable {

        /// A new episode has finished transcription. Includes the topics
        /// extracted by the per-episode fan-out (lane 6 territory).
        case episodeTranscribed(
            episodeID: UUID,
            podcastID: UUID,
            extractedTopics: [String],
            extractedPeople: [String]
        )

        /// A user contested a claim on a page; the next regen must
        /// either remove or counter-cite that claim.
        case userContestedClaim(
            pageSlug: String,
            scope: WikiScope,
            claimID: UUID
        )

        /// An episode was removed from the user's library — every page
        /// citing that episode needs its citations re-validated.
        case episodeRemoved(episodeID: UUID, podcastID: UUID)

        /// The embedding or LLM model version bumped — every page is
        /// stale and must be recompiled with the new model.
        case modelMigrated(newModel: String)
    }

    // MARK: - Outputs

    /// What the executor should do, expressed as data.
    struct WikiRefreshJob: Hashable, Sendable {
        var slug: String
        var scope: WikiScope
        var reason: Reason
        var priority: Priority

        enum Reason: String, Codable, Sendable {
            case newEvidence
            case userContested
            case episodeRemoved
            case modelMigration
        }

        enum Priority: Int, Comparable, Codable, Sendable {
            case low = 0
            case normal = 50
            case high = 100

            static func < (lhs: Priority, rhs: Priority) -> Bool {
                lhs.rawValue < rhs.rawValue
            }
        }
    }

    // MARK: - Producer

    /// Returns the jobs that should be enqueued for the supplied event,
    /// given the current inventory of pages on disk.
    ///
    /// `inventory` is read once and *not* mutated; callers pass a
    /// snapshot so the producer remains pure and deterministic.
    static func jobs(
        for event: Event,
        inventory: WikiInventory
    ) -> [WikiRefreshJob] {
        switch event {
        case .episodeTranscribed(_, let podcastID, let topics, let people):
            return jobsForNewEpisode(
                podcastID: podcastID,
                topics: topics,
                people: people,
                inventory: inventory
            )

        case .userContestedClaim(let slug, let scope, _):
            return [
                WikiRefreshJob(
                    slug: WikiPage.normalize(slug: slug),
                    scope: scope,
                    reason: .userContested,
                    priority: .high
                )
            ]

        case .episodeRemoved(_, _):
            // We can't tell from the inventory alone which pages cite a
            // given episode (citation count yes, episode IDs no), so the
            // safe move is to flag every page that has any citations at
            // low priority and let the executor open each page to check.
            return inventory.entries
                .filter { $0.citationCount > 0 }
                .map { entry in
                    WikiRefreshJob(
                        slug: entry.slug,
                        scope: entry.scope,
                        reason: .episodeRemoved,
                        priority: .low
                    )
                }
                .uniqued()

        case .modelMigrated:
            return inventory.entries.map { entry in
                WikiRefreshJob(
                    slug: entry.slug,
                    scope: entry.scope,
                    reason: .modelMigration,
                    priority: .low
                )
            }
        }
    }

    // MARK: - Implementation details

    /// New episode → fan out to one job per affected slug.
    ///
    /// Heuristic: a topic/person extracted from the new episode produces
    /// a refresh job iff a page already exists at that slug. The wiki
    /// only auto-refreshes pages the user has previously visited or the
    /// agent has previously compiled — we never *create* pages from a
    /// trigger, only refresh existing ones.
    private static func jobsForNewEpisode(
        podcastID: UUID,
        topics: [String],
        people: [String],
        inventory: WikiInventory
    ) -> [WikiRefreshJob] {
        let topicSlugs = topics.map(WikiPage.normalize(slug:))
        let peopleSlugs = people.map(WikiPage.normalize(slug:))
        let candidateSlugs = Set(topicSlugs + peopleSlugs)

        var jobs: [WikiRefreshJob] = []
        for entry in inventory.entries {
            guard candidateSlugs.contains(entry.slug) else { continue }
            // Both the global page (if it exists) and the per-podcast page
            // (if it exists) want a refresh — they have different evidence
            // pools and are not interchangeable.
            switch entry.scope {
            case .global:
                jobs.append(.init(
                    slug: entry.slug,
                    scope: .global,
                    reason: .newEvidence,
                    priority: .normal
                ))
            case .podcast(let id) where id == podcastID:
                jobs.append(.init(
                    slug: entry.slug,
                    scope: .podcast(id),
                    reason: .newEvidence,
                    priority: .high
                ))
            case .podcast:
                continue
            }
        }
        return jobs.uniqued()
    }
}

// MARK: - Internal helpers

private extension Array where Element: Hashable {
    /// Order-preserving deduplication.
    func uniqued() -> [Element] {
        var seen: Set<Element> = []
        var out: [Element] = []
        for element in self where seen.insert(element).inserted {
            out.append(element)
        }
        return out
    }
}

