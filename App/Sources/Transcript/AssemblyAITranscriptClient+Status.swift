import Foundation

enum AssemblyAIStatusObservation: Sendable {
    case pending(status: String?)
    case completed(Transcript)
}

extension AssemblyAITranscriptClient {
    /// Performs exactly one provider status read. Rust decides whether and
    /// when another recovery request is warranted.
    func observe(
        _ job: AssemblyAIJob,
        maximumResponseBytes: UInt64
    ) async throws -> AssemblyAIStatusObservation {
        try Task.checkCancellation()
        guard let key = try credential(), !key.isEmpty else {
            throw TranscribeError.missingAPIKey
        }
        let endpoint = baseURL
            .appendingPathComponent("v2")
            .appendingPathComponent("transcript")
            .appendingPathComponent(job.transcriptID)
        var request = URLRequest(url: endpoint)
        request.httpMethod = "GET"
        request.setValue(key, forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.timeoutInterval = Self.submitTimeout

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch is CancellationError {
            throw TranscribeError.cancelled
        } catch let error as URLError where error.code == .cancelled {
            throw TranscribeError.cancelled
        } catch let error as URLError where error.code == .timedOut {
            throw TranscribeError.timedOut
        }
        guard UInt64(data.count) <= maximumResponseBytes else {
            throw CoreTranscriptTransportError.responseTooLarge
        }
        try Self.assertOK(response: response, data: data)
        let payload: AssemblyAITranscriptPayload
        do {
            payload = try Self.decoder.decode(AssemblyAITranscriptPayload.self, from: data)
        } catch {
            throw TranscribeError.decoding
        }

        switch payload.status {
        case "completed":
            return .completed(Transcript.fromAssemblyAI(
                payload,
                episodeID: job.episodeID,
                languageHint: job.languageHint
            ))
        case "error":
            throw TranscribeError.remoteError
        case "queued", "processing":
            return .pending(status: payload.status)
        default:
            throw TranscribeError.invalidResponse
        }
    }
}
