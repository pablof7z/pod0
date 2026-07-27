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

    func testRecoverySurfaceDoesNotRenderInternalDiagnostics() throws {
        let sourceURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources/App/SharedCoreUnavailableView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)

        XCTAssertFalse(source.contains("Diagnostic:"))
        XCTAssertFalse(source.contains("stage:"))

        // The view now *takes* a reason so it can choose an honest remedy, but
        // it must never render one — those strings are internal failure codes
        // like "StoreNewerThanApp". Assert on interpolation rather than on the
        // parameter name: the old check banned the word "reason:" outright,
        // which conflated naming the input with displaying it.
        XCTAssertFalse(source.contains("Text(reason"))
        XCTAssertFalse(source.contains("\\(reason"))
        XCTAssertFalse(source.contains("Label(reason"))
    }

    func testAllEpisodesUsesMenuFiltersAndCollapsibleSearch() throws {
        let sourceURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources/Features/Library/AllEpisodesView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)

        XCTAssertTrue(source.contains("Picker(\"Filter episodes\""))
        XCTAssertTrue(source.contains("return \"Bookmarked\""))
        XCTAssertTrue(source.contains("if showsSearch"))
        XCTAssertTrue(source.contains(".onScrollGeometryChange"))
        XCTAssertTrue(source.contains(".navigationBarDrawer(displayMode: .automatic)"))
        XCTAssertTrue(source.contains("Color(.systemBackground)"))
        XCTAssertFalse(source.contains("Color(.systemGroupedBackground)"))
        XCTAssertFalse(source.contains("filterRailSection"))
        XCTAssertFalse(source.contains(".navigationBarDrawer(displayMode: .always)"))
    }

    func testClipsHasNoSavedSegmentsAndUsesCollapsibleSearch() throws {
        let sourceURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources/Features/Clips/ClipsView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)
        let segmentSource = try String(
            contentsOf: sourceURL
                .deletingLastPathComponent()
                .appendingPathComponent("ClipsSegment.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(segmentSource.contains("Text(\"Clips\")"))
        XCTAssertTrue(segmentSource.contains(".onScrollGeometryChange"))
        XCTAssertTrue(source.contains("if showsSearch"))
        XCTAssertTrue(source.contains(".navigationBarDrawer(displayMode: .automatic)"))
        XCTAssertTrue(source.contains("Color(.systemBackground)"))
        XCTAssertFalse(source.contains("Color(.systemGroupedBackground)"))
        XCTAssertFalse(source.contains("StarredSegment"))
        // The Saved/Starred split is what this screen dropped, and it stays
        // dropped. The Clips/Notes switch that replaced it belongs in the
        // navigation toolbar, never inline above the list where the old
        // segmented control sat.
        XCTAssertFalse(source.contains(".safeAreaInset(edge: .top)"))
        XCTAssertTrue(source.contains("ToolbarItem(placement: .principal)"))
    }

    func testSettingsEntryLivesInSidebarNotSharedToolbar() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let rootSource = try String(
            contentsOf: root.appendingPathComponent("App/Sources/App/RootView.swift"),
            encoding: .utf8
        )
        let sidebarSource = try String(
            contentsOf: root.appendingPathComponent("App/Sources/App/AppSidebarView.swift"),
            encoding: .utf8
        )

        XCTAssertFalse(rootSource.contains(".accessibilityLabel(\"Settings\")"))
        XCTAssertFalse(rootSource.contains(".sheet(isPresented: $showSettings)"))
        XCTAssertTrue(rootSource.contains("case .settings:"))
        XCTAssertTrue(sidebarSource.contains("navRow(\"Settings\""))
        XCTAssertTrue(sidebarSource.contains("selectedTab = .settings"))
    }

    func testGlobalSearchSheetAndButtonAreRemoved() throws {
        let sourceURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources/App/RootView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)

        XCTAssertFalse(source.contains("showSearch"))
        XCTAssertFalse(source.contains("searchSheet"))
        XCTAssertFalse(source.contains(".accessibilityLabel(\"Search\")"))
        XCTAssertFalse(source.contains("PodcastSearchView()"))
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


// MARK: - SharedCoreUnavailableView copy

final class SharedCoreUnavailableCopyTests: XCTestCase {

    private let blockedReasons: [SharedLibraryBootstrapFailureCode] = [
        .storeNewerThanApp, .migrationFailed, .storeUnreadable,
    ]

    func testNoBlockedReasonTellsTheUserToReopen() {
        for reason in blockedReasons {
            let message = SharedCoreUnavailableView(reason: reason.rawValue).messageForTesting
            XCTAssertFalse(
                message.localizedCaseInsensitiveContains("reopen Pod0 to try again"),
                "\(reason.rawValue) must not promise a retry that cannot work"
            )
        }
    }

    func testEveryBlockedReasonStillReassuresAboutTheData() {
        // The user's default assumption on a store failure is that their
        // library is gone. That reassurance earns its place in all of them.
        for reason in blockedReasons {
            let message = SharedCoreUnavailableView(reason: reason.rawValue).messageForTesting
            XCTAssertTrue(
                message.localizedCaseInsensitiveContains("safe")
                    || message.localizedCaseInsensitiveContains("hasn’t been changed"),
                "\(reason.rawValue) should say the data is intact"
            )
        }
    }

    func testVersionSkewNamesUpdatingAsTheRemedy() {
        let message = SharedCoreUnavailableView(
            reason: SharedLibraryBootstrapFailureCode.storeNewerThanApp.rawValue
        ).messageForTesting
        XCTAssertTrue(message.localizedCaseInsensitiveContains("update"))
    }

    func testUnknownAndTransientReasonsKeepTheRetryWording() {
        // Not every failure is permanent — recovery states genuinely do clear
        // on relaunch, so the retry wording must survive for them.
        for reason in [nil, "app_state_recovery_required", "StorageUnavailable"] {
            let message = SharedCoreUnavailableView(reason: reason).messageForTesting
            XCTAssertTrue(
                message.localizedCaseInsensitiveContains("reopen Pod0 to try again"),
                "\(reason ?? "nil") should keep the retry-safe wording"
            )
        }
    }
}
