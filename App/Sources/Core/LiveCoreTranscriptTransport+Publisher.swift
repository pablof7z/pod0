import Foundation
import Pod0Core

extension LiveCoreTranscriptTransport {
    func fetchPublisher(
        context: TranscriptCapabilityContext,
        sourceURL: String,
        mimeHint: String?,
        maximumResponseBytes: UInt64
    ) async throws -> CoreTranscriptTransportObservation {
        guard let url = URL(string: sourceURL),
              let scheme = url.scheme?.lowercased(),
              scheme == "https" || scheme == "http",
              let episodeID = context.episodeId.uuid,
              maximumResponseBytes > 0
        else { throw CoreTranscriptTransportError.invalidRequest }

        var request = URLRequest(url: url)
        request.timeoutInterval = 30
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")
        request.setValue(
            "application/json;q=1.0, text/vtt;q=0.9, application/x-subrip;q=0.9",
            forHTTPHeaderField: "Accept"
        )
        do {
            let (stream, response) = try await session.bytes(for: request)
            guard let http = response as? HTTPURLResponse,
                  (200..<300).contains(http.statusCode)
            else { throw CoreTranscriptTransportError.publisherUnavailable }
            if http.expectedContentLength > 0,
               UInt64(http.expectedContentLength) > maximumResponseBytes {
                throw CoreTranscriptTransportError.responseTooLarge
            }
            var data = Data()
            data.reserveCapacity(Int(min(maximumResponseBytes, 128 * 1_024)))
            for try await byte in stream {
                try Task.checkCancellation()
                guard UInt64(data.count) < maximumResponseBytes else {
                    throw CoreTranscriptTransportError.responseTooLarge
                }
                data.append(byte)
            }
            let transcript = try PublisherTranscriptIngestor.parse(
                data,
                mimeHint: mimeHint,
                responseMime: http.value(forHTTPHeaderField: "Content-Type"),
                url: url,
                episodeID: episodeID
            )
            return .completed(
                transcript: transcript,
                externalOperationID: nil,
                status: "completed"
            )
        } catch is CancellationError {
            throw CancellationError()
        } catch let failure as CoreTranscriptTransportError {
            throw failure
        } catch {
            throw CoreTranscriptTransportError.publisherUnavailable
        }
    }

    func enforceBound(
        _ transcript: Transcript,
        maximumResponseBytes: UInt64
    ) throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let payload = try encoder.encode(transcript)
        guard UInt64(payload.count) <= maximumResponseBytes else {
            throw CoreTranscriptTransportError.responseTooLarge
        }
    }
}
