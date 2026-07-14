import Foundation
import os.log

// MARK: - WikiRefreshExecutor

/// Runtime side of the wiki auto-refresh pipeline.
///
/// `WikiTriggers` is a pure producer — it returns `WikiRefreshJob` rows
/// describing *which* pages should be regenerated and *why*. This executor
/// is the consumer: it dedupes in-flight jobs by `(slug, scope)`, caps
/// concurrency at `maxConcurrent`, loads the prior page, calls
/// `WikiGenerator.audit(prior:)` against the user-configured wiki model,
/// and writes the result back via `WikiStorage`.
///
/// Fire-and-forget by design — callers pass jobs and the executor returns
/// immediately. Failures log to `os.log`; they do not surface to the user.
/// The user's path to a working wiki on an empty key is to open
/// `WikiGenerateSheet` and try a manual compile, which surfaces errors
/// inline.
@MainActor
final class WikiRefreshExecutor {

    // MARK: - Shared

    static let shared = WikiRefreshExecutor()

    // MARK: - Logger

    nonisolated private static let logger = Logger.app("WikiRefreshExecutor")

    // MARK: - Concurrency

    /// Hard ceiling on simultaneously running jobs. Wiki audits hit
    /// OpenRouter + the local RAG store; three at a time keeps the
    /// network and the verifier from drowning each other on a big
    /// transcript-ingest batch.
    static let maxConcurrent = 3

    // MARK: - Dependencies

    /// Storage handle used to load prior pages and persist refreshed
    /// ones. Defaults to the shared singleton; tests can swap it.
    let storage: WikiStorage

    /// Resolves the live LLM client for a given model id. Tests inject a
    /// stub that records the call and returns a canned client.
    let makeClient: @MainActor (_ model: String) -> WikiOpenRouterClient

    /// Test-injectable AppStateStore lookup. Defaults to the
    /// `RAGService.shared.appStore` reference that the app boot wires up.
    let resolveAppStore: @MainActor () -> AppStateStore?

    // MARK: - State

    private struct JobKey: Hashable {
        let slug: String
        let scopePath: String
    }

    private var inFlight: Set<JobKey> = []
    private var queue: [WikiTriggers.WikiRefreshJob] = []
    private var activeCount = 0

    // MARK: - Init

    init(
        storage: WikiStorage = .shared,
        makeClient: @escaping @MainActor (_ model: String) -> WikiOpenRouterClient = { model in
            .live(model: model)
        },
        resolveAppStore: @escaping @MainActor () -> AppStateStore? = {
            RAGService.shared.appStore
        }
    ) {
        self.storage = storage
        self.makeClient = makeClient
        self.resolveAppStore = resolveAppStore
    }

    // MARK: - Public API

    /// Enqueues a batch of refresh jobs. Duplicates of an already-running
    /// `(slug, scope)` are dropped; the rest run up to `maxConcurrent` at
    /// a time and drain a FIFO queue as slots free up.
    func run(jobs: [WikiTriggers.WikiRefreshJob]) {
        for job in jobs {
            let key = JobKey(slug: job.slug, scopePath: job.scope.pathComponent)
            guard !inFlight.contains(key) else { continue }
            inFlight.insert(key)
            if activeCount < Self.maxConcurrent {
                start(job, key: key)
            } else {
                queue.append(job)
            }
        }
    }

    // MARK: - Private

    private func start(_ job: WikiTriggers.WikiRefreshJob, key: JobKey) {
        activeCount += 1
        Task { @MainActor [weak self] in
            await self?.execute(job)
            self?.finish(key: key)
        }
    }

    private func finish(key: JobKey) {
        inFlight.remove(key)
        activeCount -= 1
        guard let next = queue.first else { return }
        queue.removeFirst()
        // The key for `next` is still in `inFlight` (we inserted it in
        // `run` when we enqueued, and we never removed it for queued
        // jobs), so go straight to `start`.
        let nextKey = JobKey(slug: next.slug, scopePath: next.scope.pathComponent)
        start(next, key: nextKey)
    }

    private func execute(_ job: WikiTriggers.WikiRefreshJob) async {
        guard let appStore = resolveAppStore() else {
            Self.logger.notice(
                "skipping refresh of \(job.slug, privacy: .public): no AppStateStore attached"
            )
            return
        }
        let model = appStore.state.settings.wikiModel
        let reference = LLMModelReference(storedID: model)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            Self.logger.info(
                "skipping refresh of \(job.slug, privacy: .public): no API key for \(reference.provider.displayName, privacy: .public)"
            )
            return
        }
        let prior: WikiPage?
        do {
            prior = try storage.read(slug: job.slug, scope: job.scope)
        } catch {
            Self.logger.error(
                "failed to read prior page for \(job.slug, privacy: .public): \(String(describing: error), privacy: .public)"
            )
            return
        }
        guard let prior else {
            // Triggers only fan out to slugs that *already exist*; if we got
            // here the inventory disagreed with the disk. Skip — the next
            // write will rebuild the inventory.
            return
        }
        let generator = WikiGenerator(
            rag: RAGService.shared.wikiRAG,
            client: makeClient(model),
            storage: storage,
            model: model
        )
        do {
            let result = try await generator.audit(prior: prior)
            try generator.persist(result.page)
            Self.logger.info(
                "refreshed \(job.slug, privacy: .public) — reason=\(job.reason.rawValue, privacy: .public), kept=\(result.keptClaims), dropped=\(result.droppedClaims)"
            )
        } catch WikiGeneratorError.insufficientEvidence {
            // Expected on freshly-removed episodes or topics that no
            // longer have RAG coverage in this scope. Refuse to clobber
            // the prior page — quiet log only.
            Self.logger.debug(
                "skipped refresh of \(job.slug, privacy: .public): no current evidence in scope"
            )
        } catch {
            Self.logger.error(
                "refresh failed for \(job.slug, privacy: .public): \(String(describing: error), privacy: .public)"
            )
        }
    }
}
