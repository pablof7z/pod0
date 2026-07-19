import Foundation

extension TranscriptIngestService {
    /// Once a resumable paid provider identity exists it is authoritative.
    /// Retrying the publisher fallback first could overwrite that identity
    /// and turn a safe poll into a duplicate provider submission after a kill.
    nonisolated static func shouldAttemptPublisher(
        userInitiated: Bool,
        externalProvider: String?,
        externalOperationID: String?
    ) -> Bool {
        guard !userInitiated else { return false }
        return (externalProvider == nil && externalOperationID == nil)
            || externalProvider == "publisherTranscript"
    }

    /// Resolves durable provider evidence without ever laundering a
    /// mismatched or half-recorded submission into a fresh paid request.
    nonisolated static func resumableExternalOperationID(
        expectedProvider: String,
        recordedProvider: String?,
        recordedID: String?
    ) throws -> String? {
        if recordedProvider == nil, recordedID == nil { return nil }
        if recordedProvider == "publisherTranscript" { return nil }
        guard recordedProvider == expectedProvider,
              let recordedID,
              !recordedID.isBlank else {
            throw JobFailure(
                classification: .unsafeToRetry,
                message: "Recorded provider identity does not match the transcript executor."
            )
        }
        return recordedID
    }

    nonisolated static func externalProviderName(for provider: STTProvider) -> String {
        switch provider {
        case .assemblyAI: "assemblyAI"
        case .elevenLabsScribe: "elevenLabsScribe"
        case .openRouterWhisper: "openRouterWhisper"
        case .appleNative: "appleNative"
        }
    }
}
