import Foundation

enum UserFacingRecoveryAction: Equatable, Sendable {
    case retry
    case openProviders
}

struct UserFacingFailure: Equatable, Sendable {
    let code: ProductFailureCode
    let title: String
    let message: String
    let recoveryAction: UserFacingRecoveryAction?
    let diagnosticID: String?
}

enum UserFacingFailurePresenter {
    static func make(
        failure: ProductFailure,
        canRetry: Bool = false,
        canOpenProviders: Bool = false
    ) -> UserFacingFailure {
        let failure = failure.code == .unexpected && failure.diagnosticID == nil
            ? ProductFailure(code: .unexpected, diagnosticID: ProductFailure.makeDiagnosticID())
            : failure
        let recoveryAction = recoveryAction(
            for: failure.code,
            canRetry: canRetry,
            canOpenProviders: canOpenProviders
        )
        return UserFacingFailure(
            code: failure.code,
            title: title(for: failure.code),
            message: message(
                for: failure.code,
                canRetry: recoveryAction == .retry,
                canOpenProviders: recoveryAction == .openProviders,
                diagnosticID: failure.diagnosticID
            ),
            recoveryAction: recoveryAction,
            diagnosticID: failure.diagnosticID
        )
    }

    static func make(
        error: Error,
        canRetry: Bool = false,
        canOpenProviders: Bool = false
    ) -> UserFacingFailure {
        make(
            failure: ProductFailure.classify(error),
            canRetry: canRetry,
            canOpenProviders: canOpenProviders
        )
    }

    static func make(
        stableCode: String,
        diagnosticID: String? = nil,
        canRetry: Bool = false
    ) -> UserFacingFailure {
        let code = ProductFailureCode(rawValue: stableCode) ?? .unexpected
        return make(
            failure: ProductFailure(code: code, diagnosticID: diagnosticID),
            canRetry: canRetry
        )
    }

    static func make(job: WorkflowJobProjection) -> UserFacingFailure {
        let code = job.lastErrorClass?.productFailureCode ?? .unexpected
        let diagnosticID = String(job.id.uuidString.prefix(8)).uppercased()
        return make(
            failure: ProductFailure(code: code, diagnosticID: diagnosticID),
            canRetry: job.allowedActions.contains(.retry),
            canOpenProviders: job.lastErrorClass == .missingCredential
        )
    }

    private static func recoveryAction(
        for code: ProductFailureCode,
        canRetry: Bool,
        canOpenProviders: Bool
    ) -> UserFacingRecoveryAction? {
        if code == .missingCredential, canOpenProviders { return .openProviders }
        guard canRetry else { return nil }
        switch code {
        case .rateLimited, .offline, .network, .corruptArtifact, .unexpected:
            return .retry
        case .missingCredential, .permissionDenied, .unsupportedFormat, .providerRecovery, .cancelled,
             .invalidInput, .missingDependency:
            return nil
        }
    }

    private static func title(for code: ProductFailureCode) -> String {
        switch code {
        case .missingCredential:
            localized("failure.missing_credential.title", fallback: "Connect a provider")
        case .permissionDenied:
            localized("failure.permission_denied.title", fallback: "Permission required")
        case .rateLimited:
            localized("failure.rate_limited.title", fallback: "Provider is busy")
        case .offline:
            localized("failure.offline.title", fallback: "You're offline")
        case .network:
            localized("failure.network.title", fallback: "Connection interrupted")
        case .unsupportedFormat:
            localized("failure.unsupported_format.title", fallback: "Format not supported")
        case .providerRecovery:
            localized("failure.provider_recovery.title", fallback: "Recovery needed")
        case .corruptArtifact:
            localized("failure.corrupt_artifact.title", fallback: "Saved result is damaged")
        case .cancelled:
            localized("failure.cancelled.title", fallback: "Cancelled")
        case .invalidInput:
            localized("failure.invalid_input.title", fallback: "Source needs attention")
        case .missingDependency:
            localized("failure.missing_dependency.title", fallback: "Waiting for an earlier step")
        case .unexpected:
            localized("failure.unexpected.title", fallback: "Something went wrong")
        }
    }

    private static func message(
        for code: ProductFailureCode,
        canRetry: Bool,
        canOpenProviders: Bool,
        diagnosticID: String?
    ) -> String {
        let base: String
        switch code {
        case .missingCredential:
            base = canOpenProviders
                ? localized(
                    "failure.missing_credential.open_providers",
                    fallback: "Connect the required provider to continue."
                )
                : localized(
                    "failure.missing_credential.message",
                    fallback: "A provider connection is required before this work can continue."
                )
        case .permissionDenied:
            base = localized(
                "failure.permission_denied.message",
                fallback: "Allow the required access in Settings, then try again."
            )
        case .rateLimited:
            base = canRetry
                ? localized(
                    "failure.rate_limited.retry",
                    fallback: "The provider is limiting requests. Wait a moment, then retry."
                )
                : localized(
                    "failure.rate_limited.message",
                    fallback: "The provider is limiting requests right now."
                )
        case .offline:
            base = canRetry
                ? localized(
                    "failure.offline.retry",
                    fallback: "Reconnect to the internet, then retry."
                )
                : localized("failure.offline.message", fallback: "Pod0 could not reach the internet.")
        case .network:
            base = canRetry
                ? localized(
                    "failure.network.retry",
                    fallback: "The connection was interrupted. Check your connection, then retry."
                )
                : localized(
                    "failure.network.message",
                    fallback: "The connection was interrupted before this work finished."
                )
        case .unsupportedFormat:
            base = localized(
                "failure.unsupported_format.message",
                fallback: "The selected provider cannot process this source format."
            )
        case .providerRecovery:
            base = localized(
                "failure.provider_recovery.message",
                fallback: "Pod0 paused after an interrupted provider submission to avoid duplicate work."
            )
        case .corruptArtifact:
            base = canRetry
                ? localized(
                    "failure.corrupt_artifact.retry",
                    fallback: "The saved result failed verification. Retry to rebuild it safely."
                )
                : localized(
                    "failure.corrupt_artifact.message",
                    fallback: "The saved result failed verification and was not used."
                )
        case .cancelled:
            base = localized("failure.cancelled.message", fallback: "This work was cancelled.")
        case .invalidInput:
            base = localized(
                "failure.invalid_input.message",
                fallback: "The current source or setup cannot be used for this work."
            )
        case .missingDependency:
            base = localized(
                "failure.missing_dependency.message",
                fallback: "An earlier required step is not ready yet."
            )
        case .unexpected:
            base = canRetry
                ? localized(
                    "failure.unexpected.retry",
                    fallback: "Pod0 stopped safely. Retry when you're ready."
                )
                : localized(
                    "failure.unexpected.message",
                    fallback: "Pod0 stopped safely without using an incomplete result."
                )
        }
        guard code == .unexpected, let diagnosticID else { return base }
        let label = localized("failure.diagnostic_id", fallback: "Diagnostic")
        return "\(base) \(label) \(diagnosticID)."
    }

    private static func localized(_ key: String, fallback: String) -> String {
        Bundle.main.localizedString(
            forKey: key,
            value: fallback,
            table: "FailurePresentation"
        )
    }
}
