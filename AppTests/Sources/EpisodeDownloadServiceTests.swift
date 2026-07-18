import XCTest
@testable import Podcastr

@MainActor
final class EpisodeDownloadServiceTests: XCTestCase {

    func testDownloadOriginPrioritiesAreDeterministic() {
        XCTAssertEqual(DownloadIntentOrigin.user.priority, 100)
        XCTAssertEqual(DownloadIntentOrigin.playback.priority, 80)
        XCTAssertEqual(DownloadIntentOrigin.autoDownload.priority, 20)
    }

    func testAdmissionWaitsForOfflineAndLowStorageWithoutConsumingTransferSlot() {
        let policy = DownloadAdmissionPolicy()
        let automatic = AutoDownloadPolicy(mode: .allNew, wifiOnly: false)

        XCTAssertEqual(
            policy.evaluate(
                origin: .user,
                automaticPolicy: automatic,
                network: .unavailable,
                availableStorageCapacity: nil
            ),
            .wait(reason: "Download is waiting for a network connection.")
        )
        XCTAssertEqual(
            policy.evaluate(
                origin: .playback,
                automaticPolicy: automatic,
                network: .wifi,
                availableStorageCapacity: DownloadAdmissionPolicy.minimumFreeCapacity - 1
            ),
            .wait(reason: "Download is waiting for more free storage.")
        )
    }

    func testAutomaticAdmissionTracksCurrentPolicyAndWiFi() {
        let policy = DownloadAdmissionPolicy()
        XCTAssertEqual(
            policy.evaluate(
                origin: .autoDownload,
                automaticPolicy: AutoDownloadPolicy(mode: .off, wifiOnly: false),
                network: .wifi,
                availableStorageCapacity: nil
            ),
            .obsolete
        )
        XCTAssertEqual(
            policy.evaluate(
                origin: .autoDownload,
                automaticPolicy: AutoDownloadPolicy(mode: .allNew, wifiOnly: true),
                network: .other,
                availableStorageCapacity: nil
            ),
            .wait(reason: "Automatic download is waiting for Wi-Fi.")
        )
        XCTAssertEqual(
            policy.evaluate(
                origin: .autoDownload,
                automaticPolicy: AutoDownloadPolicy(mode: .allNew, wifiOnly: true),
                network: .wifi,
                availableStorageCapacity: DownloadAdmissionPolicy.minimumFreeCapacity
            ),
            .admit
        )
    }

    func testTargetedCancellationReleasesTransferOwnershipExactlyOnce() {
        let service = EpisodeDownloadService.shared
        let episodeID = UUID()
        let jobID = UUID()
        let task = service.session.downloadTask(
            with: URL(string: "https://example.com/cancel-once.mp3")!
        )
        service.episodeIDToTask[episodeID] = task
        service.taskIDToEpisodeID[task.taskIdentifier] = episodeID
        service.taskIDToJobID[task.taskIdentifier] = jobID
        service.taskIDToInputVersion[task.taskIdentifier] = "v1"

        XCTAssertTrue(service.cancelAdmittedTransfer(
            jobID: jobID,
            episodeID: episodeID
        ))
        XCTAssertFalse(service.cancelAdmittedTransfer(
            jobID: jobID,
            episodeID: episodeID
        ))
        XCTAssertNil(service.episodeIDToTask[episodeID])
        XCTAssertNil(service.taskIDToJobID[task.taskIdentifier])
        XCTAssertNil(service.taskIDToInputVersion[task.taskIdentifier])
    }

    func testBackgroundURLSessionCompletionHandlerRunsWhenSessionFinishesEvents() {
        let service = EpisodeDownloadService.shared
        var didCallCompletion = false

        service.handleEventsForBackgroundURLSession(
            identifier: EpisodeDownloadService.backgroundSessionIdentifier
        ) {
            didCallCompletion = true
        }
        service.handleBackgroundEventsFinished(for: service.session)

        XCTAssertTrue(didCallCompletion)
    }

    func testUnknownBackgroundURLSessionIdentifierCompletesImmediately() {
        let service = EpisodeDownloadService.shared
        var didCallCompletion = false

        service.handleEventsForBackgroundURLSession(identifier: "other.session") {
            didCallCompletion = true
        }

        XCTAssertTrue(didCallCompletion)
    }

    func testLaunchReconciliationAttachesCancelsAndRequeuesDeterministically() throws {
        let stateURL = AppStateTestSupport.uniqueTempFileURL()
        let databaseURL = Persistence.episodeStoreURL(for: stateURL)
        defer { AppStateTestSupport.disposeIsolatedStore(at: stateURL) }
        let store = JobStore(fileURL: databaseURL)
        let missingTaskEpisode = UUID()
        let attachedEpisode = UUID()
        _ = try store.ensureJob(downloadJob(
            key: "missing-task", subject: missingTaskEpisode, priority: 100
        ))
        _ = try store.ensureJob(downloadJob(
            key: "attached", subject: attachedEpisode, priority: 10
        ))
        let missingTask = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .download,
            capacity: 1,
            now: Date(),
            owner: "launch",
            leaseDuration: 60
        ).first)
        let attached = try XCTUnwrap(store.job(idempotencyKey: "attached"))
        let orphanID = UUID()

        let actions = DownloadReconciliationPlanner().plan(
            tasks: [
                .init(taskIdentifier: 20, jobID: orphanID, episodeID: UUID()),
                .init(taskIdentifier: 10, jobID: attached.id, episodeID: attachedEpisode),
            ],
            jobs: try store.allJobs()
        )

        XCTAssertEqual(actions, [
            .attach(taskIdentifier: 10, jobID: attached.id, episodeID: attachedEpisode),
            .cancelOrphan(taskIdentifier: 20),
            .requeueMissingTask(jobID: missingTask.id),
        ])
    }

    func testTaskDescriptionRoundTripsDurableVersionIdentity() throws {
        let jobID = UUID()
        let episodeID = UUID()
        let inputVersion = String(repeating: "a", count: 64)
        let encoded = EpisodeDownloadService.taskDescription(
            jobID: jobID,
            episodeID: episodeID,
            inputVersion: inputVersion
        )

        let parsed = try XCTUnwrap(EpisodeDownloadService.parseTaskDescription(encoded))

        XCTAssertEqual(parsed.jobID, jobID)
        XCTAssertEqual(parsed.episodeID, episodeID)
        XCTAssertEqual(parsed.inputVersion, inputVersion)
    }

    func testDownloadOutputIsAttemptStagedAndVersionVerifiedBeforePromotion() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = try EpisodeDownloadStore(rootDirectory: root)
        let episode = Episode(
            podcastID: UUID(), guid: "staged", title: "Staged", pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
        let source = root.appendingPathComponent("urlsession.tmp")
        let bytes = Data("attempt-scoped-audio".utf8)
        try bytes.write(to: source)
        let jobID = UUID()

        let staged = try disk.stage(
            source,
            episode: episode,
            jobID: jobID,
            inputVersion: "audio-v1"
        )

        XCTAssertNil(disk.verifiedStagedOutput(
            episodeID: episode.id,
            jobID: jobID,
            inputVersion: "audio-v2"
        ))
        let verified = try XCTUnwrap(disk.verifiedStagedOutput(
            episodeID: episode.id,
            jobID: jobID,
            inputVersion: "audio-v1",
            contentHash: ArtifactRepository.hash(bytes)
        ))
        XCTAssertEqual(verified.jobID, staged.jobID)
        XCTAssertEqual(verified.inputVersion, staged.inputVersion)
        XCTAssertEqual(verified.contentHash, staged.contentHash)
        XCTAssertEqual(verified.fileURL, staged.fileURL)
        let selected = try disk.promote(verified, episode: episode)
        XCTAssertEqual(try Data(contentsOf: selected), bytes)
        XCTAssertTrue(selected.lastPathComponent.contains(verified.contentHash))
        XCTAssertTrue(FileManager.default.fileExists(atPath: verified.fileURL.path))
    }

    func testSecondIntentCompletesFromSameVerifiedDownloadWithoutDuplicateTransfer() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("shared-download-\(UUID().uuidString).mp3")
        defer { try? FileManager.default.removeItem(at: file) }
        let bytes = Data("one-transfer-two-intents".utf8)
        try bytes.write(to: file)
        let episode = Episode(
            podcastID: UUID(),
            guid: "shared-transfer",
            title: "Shared transfer",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/shared.mp3")!
        )
        made.store.mutateState { $0.episodes = [episode] }
        let database = made.store.persistence.episodeStore.fileURL
        let jobs = JobStore(fileURL: database)
        let artifacts = ArtifactRepository(fileURL: database)
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        let hash = ArtifactRepository.hash(bytes)
        try artifacts.adopt(ArtifactRecord(
            kind: .downloadFile,
            subjectID: episode.id,
            inputVersion: inputVersion,
            outputVersion: hash,
            contentHash: hash,
            location: file.path,
            origin: "existing-transfer",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date()
        ))
        _ = try jobs.ensureJob(DesiredJob(
            idempotencyKey: "download:\(episode.id):\(inputVersion):user",
            kind: .download,
            subjectID: episode.id,
            inputVersion: inputVersion,
            occurrenceID: "download:\(episode.id):\(inputVersion):user",
            resourceClass: .download
        ))
        let claimed = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: .download,
            capacity: 1,
            now: Date(),
            owner: "second-intent",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(claimed.leaseToken)
        try jobs.markRunning(id: claimed.id, leaseToken: token)

        let committed = try await WorkflowArtifactVerifier(
            appStore: made.store,
            artifacts: artifacts
        ).verifyAndCommit(claimed, leaseToken: token, outputVersion: hash)

        XCTAssertTrue(committed)
        XCTAssertEqual(try jobs.job(id: claimed.id)?.state, .succeeded)
        guard case .downloaded(let selected, let size) = made.store.episode(id: episode.id)?.downloadState else {
            return XCTFail("Verified shared output was not projected as downloaded")
        }
        XCTAssertEqual(selected, file)
        XCTAssertEqual(size, Int64(bytes.count))
    }

    func testCancelledQueuedDownloadCannotBeClaimedOrExecuted() async throws {
        let stateURL = AppStateTestSupport.uniqueTempFileURL()
        let databaseURL = Persistence.episodeStoreURL(for: stateURL)
        defer { AppStateTestSupport.disposeIsolatedStore(at: stateURL) }
        let jobs = JobStore(fileURL: databaseURL)
        let subject = UUID()
        let desired = downloadJob(
            key: "download:\(subject):v1:user-cancelled-before-start",
            subject: subject,
            priority: 100
        )
        _ = try jobs.ensureJob(desired)
        try jobs.cancelActiveJobs(kind: .download, subjectID: subject)
        let executor = DownloadExecutionProbe()
        let coordinator = WorkCoordinator(
            jobStore: jobs,
            executors: [.download: executor],
            capacities: [.download: 1]
        )

        await coordinator.drainDueJobs()

        let runCount = await executor.runCount
        XCTAssertEqual(runCount, 0)
        XCTAssertEqual(
            try jobs.job(idempotencyKey: desired.idempotencyKey)?.state,
            .cancelled
        )
        XCTAssertTrue(try jobs.claimDueJobs(
            resourceClass: .download,
            capacity: 1,
            now: Date(),
            owner: "cancel-verifier",
            leaseDuration: 60
        ).isEmpty)
    }

    func testDismissedDownloadFailureIsHiddenButRemainsRetryable() throws {
        let stateURL = AppStateTestSupport.uniqueTempFileURL()
        let databaseURL = Persistence.episodeStoreURL(for: stateURL)
        defer { AppStateTestSupport.disposeIsolatedStore(at: stateURL) }
        let jobs = JobStore(fileURL: databaseURL)
        let subject = UUID()
        let desired = downloadJob(
            key: "download:\(subject):v1:user-failed",
            subject: subject,
            priority: 100
        )
        _ = try jobs.ensureJob(desired)
        let claimed = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: .download,
            capacity: 1,
            now: Date(),
            owner: "failure-test",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(claimed.leaseToken)
        try jobs.markRunning(id: claimed.id, leaseToken: token)
        try jobs.markFailedPermanent(
            id: claimed.id,
            leaseToken: token,
            error: JobFailure(classification: .unexpected, message: "failed")
        )

        try jobs.dismissJobsNeedingAttention(kind: .download, subjectID: subject)

        let dismissed = try XCTUnwrap(jobs.job(idempotencyKey: desired.idempotencyKey))
        XCTAssertEqual(dismissed.state, .cancelled)
        XCTAssertEqual(dismissed.lastErrorClass, .cancelled)
        XCTAssertEqual(dismissed.lastErrorMessage, "Dismissed by user")

        try jobs.rearmJob(idempotencyKey: desired.idempotencyKey)
        XCTAssertEqual(
            try jobs.job(idempotencyKey: desired.idempotencyKey)?.state,
            .pending
        )
    }

    func testManualDownloadIntentRearmsSucceededRowAfterLocalArtifactRemoval() throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let episode = Episode(
            podcastID: UUID(),
            guid: "manual-redownload",
            title: "Manual re-download",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/manual-redownload.mp3")!
        )
        made.store.upsertEpisodes([episode], forPodcast: episode.podcastID)

        let runtime = WorkflowRuntime.shared
        let initial = try runtime.persistDownloadIntent(episodeID: episode.id, origin: .user)
        let jobs = try XCTUnwrap(runtime.jobStore)
        let claimed = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: .download,
            capacity: 1,
            now: Date(),
            owner: "redownload-test",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(claimed.leaseToken)
        try jobs.markRunning(id: initial.id, leaseToken: token)
        try jobs.complete(id: initial.id, leaseToken: token, outputVersion: initial.inputVersion)
        XCTAssertEqual(try jobs.job(idempotencyKey: initial.idempotencyKey)?.state, .succeeded)

        let rearmed = try runtime.persistDownloadIntent(episodeID: episode.id, origin: .user)

        XCTAssertEqual(rearmed.id, initial.id)
        XCTAssertEqual(rearmed.state, .pending)
        XCTAssertEqual(rearmed.attempt, 0)
    }

    private func downloadJob(key: String, subject: UUID, priority: Int) -> DesiredJob {
        DesiredJob(
            idempotencyKey: key,
            kind: .download,
            subjectID: subject,
            inputVersion: "v1",
            occurrenceID: key,
            priority: priority,
            resourceClass: .download
        )
    }
}

private actor DownloadExecutionProbe: JobExecutor {
    private(set) var runCount = 0

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        runCount += 1
        return .succeeded(outputVersion: context.job.inputVersion)
    }
}
