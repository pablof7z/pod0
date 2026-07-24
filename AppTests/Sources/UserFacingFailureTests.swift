import Foundation
import XCTest
@testable import Podcastr

final class UserFacingFailureTests: XCTestCase {
    func testEveryStableFailureCodeHasLocalizedFallbackCopy() {
        for code in ProductFailureCode.allCases {
            let presented = UserFacingFailurePresenter.make(
                stableCode: code.rawValue,
                diagnosticID: "ABC12345"
            )
            XCTAssertEqual(presented.code, code)
            XCTAssertFalse(presented.title.isEmpty, "Missing title for \(code)")
            XCTAssertFalse(presented.message.isEmpty, "Missing message for \(code)")
            XCTAssertFalse(presented.title.hasPrefix("failure."))
            XCTAssertFalse(presented.message.hasPrefix("failure."))
        }
        let future = UserFacingFailurePresenter.make(stableCode: "futureFailure")
        XCTAssertEqual(future.code, .unexpected)
        XCTAssertNotNil(future.diagnosticID)
        XCTAssertTrue(future.message.contains(future.diagnosticID ?? "missing"))
    }

    func testRecoveryCopyAppearsOnlyWhenTheTypedCapabilityAllowsIt() {
        let network = ProductFailure(code: .network)
        let withoutRetry = UserFacingFailurePresenter.make(failure: network)
        XCTAssertNil(withoutRetry.recoveryAction)
        XCTAssertFalse(withoutRetry.message.localizedCaseInsensitiveContains("retry"))

        let withRetry = UserFacingFailurePresenter.make(failure: network, canRetry: true)
        XCTAssertEqual(withRetry.recoveryAction, .retry)
        XCTAssertTrue(withRetry.message.localizedCaseInsensitiveContains("retry"))

        let unsupported = UserFacingFailurePresenter.make(
            failure: ProductFailure(code: .unsupportedFormat),
            canRetry: true
        )
        XCTAssertNil(unsupported.recoveryAction)
        XCTAssertFalse(unsupported.message.localizedCaseInsensitiveContains("retry"))

        let disconnected = UserFacingFailurePresenter.make(
            failure: ProductFailure(code: .missingCredential)
        )
        XCTAssertNil(disconnected.recoveryAction)
        let connectable = UserFacingFailurePresenter.make(
            failure: ProductFailure(code: .missingCredential),
            canOpenProviders: true
        )
        XCTAssertEqual(connectable.recoveryAction, .openProviders)
    }

    func testTypedProviderFailuresMapWithoutRenderingRawBodiesOrInternals() {
        let cases: [(Error, ProductFailureCode)] = [
            (ElevenLabsScribeClient.ScribeError.http(status: 401), .missingCredential),
            (ElevenLabsScribeClient.ScribeError.http(status: 429), .rateLimited),
            (AssemblyAITranscriptClient.TranscribeError.http(status: 422), .unsupportedFormat),
            (OpenRouterWhisperClient.WhisperError.timedOut, .network),
            (URLError(.notConnectedToInternet), .offline),
            (CancellationError(), .cancelled),
        ]
        for (error, expectedCode) in cases {
            let failure = ProductFailure.classify(error, diagnosticID: "SAFE1234")
            XCTAssertEqual(failure.code, expectedCode)
            let presented = UserFacingFailurePresenter.make(failure: failure, canRetry: true)
            let rendered = "\(presented.title) \(presented.message)"
            XCTAssertFalse(rendered.contains("SECRET"))
            XCTAssertFalse(rendered.contains("/private"))
            XCTAssertFalse(rendered.contains("request-id"))
            XCTAssertFalse(rendered.contains("token="))
        }
    }

    func testWorkflowFailureProjectionUsesCodeAndAllowedActionsNotRawMessage() {
        for errorClass in JobErrorClass.allCases {
            let projection = makeProjection(errorClass: errorClass)
            let presented = UserFacingFailurePresenter.make(job: projection)
            XCTAssertEqual(presented.code, errorClass.productFailureCode)
            XCTAssertFalse(presented.message.contains("SECRET"))
            XCTAssertEqual(
                presented.recoveryAction == .retry,
                projection.allowedActions.contains(.retry)
                    && [.rateLimited, .offline, .network, .corruptArtifact, .unexpected]
                        .contains(errorClass.productFailureCode)
            )
        }
    }

    func testFeaturePresentationDoesNotBindRawFailureStrings() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources/Features")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: nil
        ))
        let forbidden = [
            "= error.localizedDescription",
            "Text(error.localizedDescription",
            "Label(error.localizedDescription",
            "job.lastErrorMessage",
            "run.failureReason",
        ]
        var violations: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            let source = try String(contentsOf: file, encoding: .utf8)
            for pattern in forbidden where source.contains(pattern) {
                violations.append("\(file.lastPathComponent): \(pattern)")
            }
        }
        XCTAssertTrue(violations.isEmpty, violations.joined(separator: "\n"))
    }

    private func makeProjection(errorClass: JobErrorClass) -> WorkflowJobProjection {
        let now = Date()
        return WorkflowJobProjection(job: WorkJob(
            id: UUID(), idempotencyKey: UUID().uuidString, kind: .transcriptIngest,
            subjectID: UUID(), inputVersion: "v1", occurrenceID: nil,
            payloadVersion: 1, payload: nil, state: .failedPermanent, priority: 0,
            resourceClass: .remoteSTT, attempt: 1, maxAttempts: 8,
            notBefore: now, leaseToken: nil, leaseOwner: nil, leaseExpiresAt: nil,
            externalProvider: "provider", externalOperationID: "request-id",
            externalOperationState: nil, outputVersion: nil,
            lastErrorClass: errorClass,
            lastErrorMessage: "SECRET body /private/file token=request-id",
            createdAt: now, updatedAt: now
        ))
    }
}
