import Foundation
import XCTest
@testable import Podcastr

final class EpisodePreparationStatusTests: XCTestCase {
    func testEveryVisibleLifecycleHasTruthfulPresentation() throws {
        let expected: [WorkJobState: (String, EpisodePreparationStatus.Tone)] = [
            .pending: ("Transcript queued", .working),
            .leased: ("Transcript queued", .working),
            .running: ("Preparing transcript", .working),
            .retryScheduled: ("Transcript retry scheduled", .working),
            .blocked: ("Transcript waiting for setup", .attention),
            .failedPermanent: ("Transcript needs attention", .attention),
            .cancelled: ("Transcript paused", .quiet),
            .succeeded: ("Transcript ready", .quiet),
        ]
        for (state, expectation) in expected {
            let status = try XCTUnwrap(EpisodePreparationPresenter.make(
                episode: episode(),
                jobs: [projection(state: state)]
            ))
            XCTAssertEqual(status.title, expectation.0, "Unexpected title for \(state)")
            XCTAssertEqual(status.tone, expectation.1, "Unexpected tone for \(state)")
            XCTAssertFalse(status.message.isEmpty)
        }
    }

    func testBlockersOfferTheSafeNextStepWithoutLeakingRawErrors() throws {
        let credential = try XCTUnwrap(EpisodePreparationPresenter.make(
            episode: episode(),
            jobs: [projection(
                state: .blocked,
                errorClass: .missingCredential,
                rawMessage: "SECRET provider body"
            )]
        ))
        XCTAssertEqual(credential.actions, [.openProviders, .retry])
        XCTAssertFalse(credential.message.contains("SECRET"))

        let dependency = try XCTUnwrap(EpisodePreparationPresenter.make(
            episode: episode(),
            jobs: [projection(state: .blocked, errorClass: .missingDependency)]
        ))
        XCTAssertEqual(dependency.actions, [.downloadEpisode, .retry])

        let unsafe = try XCTUnwrap(EpisodePreparationPresenter.make(
            episode: episode(),
            jobs: [projection(state: .blocked, errorClass: .unsafeToRetry)]
        ))
        XCTAssertFalse(unsafe.actions.contains(.retry))
        XCTAssertTrue(unsafe.message.contains("avoid duplicate work"))
    }

    func testReadyTranscriptProducesStableReadyResult() throws {
        let ready = episode(transcriptState: .ready(source: .publisher))
        let status = try XCTUnwrap(EpisodePreparationPresenter.make(episode: ready, jobs: []))
        XCTAssertEqual(status.title, "Ready to recall")
        XCTAssertEqual(status.tone, .ready)
        XCTAssertTrue(status.actions.isEmpty)
    }

    func testProviderPhaseUsesBoundedKnownCopy() throws {
        let known = try XCTUnwrap(EpisodePreparationPresenter.make(
            episode: episode(),
            jobs: [projection(state: .running, externalState: "processing")]
        ))
        XCTAssertEqual(known.message, "The provider is processing the audio.")
        let unknown = try XCTUnwrap(EpisodePreparationPresenter.make(
            episode: episode(),
            jobs: [projection(state: .running, externalState: "raw-secret-state")]
        ))
        XCTAssertFalse(unknown.message.contains("raw-secret-state"))
    }

    private func episode(
        transcriptState: TranscriptState = .none
    ) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: UUID().uuidString,
            title: "Episode",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/episode.mp3")!,
            transcriptState: transcriptState
        )
    }

    private func projection(
        state: WorkJobState,
        errorClass: JobErrorClass? = nil,
        rawMessage: String? = nil,
        externalState: String? = nil
    ) -> WorkflowJobProjection {
        let now = Date()
        return WorkflowJobProjection(job: WorkJob(
            id: UUID(), idempotencyKey: UUID().uuidString, kind: .transcriptIngest,
            subjectID: UUID(), inputVersion: "v1", occurrenceID: nil,
            payloadVersion: 1, payload: nil, state: state, priority: 0,
            resourceClass: .remoteSTT, attempt: 1, maxAttempts: 8,
            notBefore: now, leaseToken: nil, leaseOwner: nil,
            leaseExpiresAt: nil, externalProvider: "provider",
            externalOperationID: nil, externalOperationState: externalState,
            outputVersion: nil, lastErrorClass: errorClass,
            lastErrorMessage: rawMessage, createdAt: now, updatedAt: now
        ))
    }
}
