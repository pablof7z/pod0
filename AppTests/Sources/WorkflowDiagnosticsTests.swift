import Foundation
import XCTest
@testable import Podcastr

final class WorkflowDiagnosticsTests: XCTestCase {
    func testEveryWorkflowKindAndStateHasExhaustivePresentation() {
        let kindTitles = WorkflowProjectionKind.allCases.map(
            WorkflowDiagnosticPresenter.kindTitle
        )
        let kindIcons = WorkflowProjectionKind.allCases.map(
            WorkflowDiagnosticPresenter.kindIcon
        )
        XCTAssertEqual(Set(kindTitles).count, WorkflowProjectionKind.allCases.count)
        XCTAssertEqual(Set(kindIcons).count, WorkflowProjectionKind.allCases.count)
        XCTAssertTrue(WorkJobKind.allCases.allSatisfy {
            WorkflowProjectionKind(rawValue: $0.rawValue) != nil
        })
        XCTAssertTrue(kindTitles.allSatisfy { !$0.isEmpty })
        XCTAssertTrue(WorkJobState.allCases.allSatisfy {
            !WorkflowDiagnosticPresenter.stateTitle($0).isEmpty
        })
        XCTAssertTrue(JobErrorClass.allCases.allSatisfy {
            !WorkflowDiagnosticPresenter.errorTitle($0).isEmpty
        })
    }

    func testDiagnosticSnapshotOmitsSensitiveWorkflowFields() {
        let job = projection(
            state: .failedPermanent,
            externalState: "secret-provider-state",
            errorMessage: "/private/path?token=secret"
        )
        let snapshot = WorkflowDiagnosticPresenter.make(job: job)
        let rendered = [
            snapshot.kindTitle,
            snapshot.stateTitle,
            snapshot.detail,
            snapshot.metadata,
            snapshot.classification ?? "",
        ].joined(separator: " ")
        XCTAssertFalse(rendered.contains("secret-provider-state"))
        XCTAssertFalse(rendered.contains("/private/path"))
        XCTAssertFalse(rendered.contains("token="))
        XCTAssertEqual(snapshot.actions, [.retry])
    }

    func testEpisodeAuditPresentationRedactsURLsErrorsPathsAndTaskTokens() {
        let event = EpisodeAuditEvent(
            episodeID: UUID(),
            kind: .downloadFailed,
            severity: .failure,
            summary: "secret raw provider body",
            details: [
                .init("URL", "https://media.example.com/audio?token=secret"),
                .init("Error", "/private/path leaked token"),
                .init("File", "/private/path/file.mp3"),
                .init("Task ID", "internal-token"),
                .init("HTTP status", "503"),
            ]
        )
        XCTAssertFalse(EpisodeAuditPresentation.summary(for: event).contains("secret"))
        let details = EpisodeAuditPresentation.details(for: event)
        XCTAssertEqual(details, [
            .init("Host", "media.example.com"),
            .init("HTTP status", "503"),
        ])
    }

    private func projection(
        state: WorkJobState,
        externalState: String?,
        errorMessage: String?
    ) -> WorkflowJobProjection {
        let now = Date()
        return WorkflowJobProjection(job: WorkJob(
            id: UUID(), idempotencyKey: "diagnostic", kind: .transcriptIngest,
            subjectID: UUID(), inputVersion: "v1", occurrenceID: nil,
            payloadVersion: 1, payload: nil, state: state, priority: 0,
            resourceClass: .remoteSTT, attempt: 2, maxAttempts: 8,
            notBefore: now, leaseToken: UUID(), leaseOwner: "secret-owner",
            leaseExpiresAt: now, externalProvider: "secret-provider",
            externalOperationID: "secret-operation-id",
            externalOperationState: externalState, outputVersion: nil,
            lastErrorClass: .unexpected, lastErrorMessage: errorMessage,
            createdAt: now, updatedAt: now
        ))
    }
}
