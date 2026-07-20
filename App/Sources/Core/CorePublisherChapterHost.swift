import Foundation
import Pod0Core

protocol CorePublisherChapterHosting: Sendable {
    func fetch(
        episodeID: EpisodeId,
        sourceURL: String,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation
}

/// Executes the bounded HTTP primitive and reports raw response facts. Rust
/// owns status policy, retry, parsing, provenance, selection, and persistence.
struct CorePublisherChapterHost: CorePublisherChapterHosting, Sendable {
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func fetch(
        episodeID: EpisodeId,
        sourceURL: String,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        guard maximumResponseBytes > 0,
              let url = URL(string: sourceURL),
              let scheme = url.scheme?.lowercased(),
              scheme == "https" || scheme == "http"
        else {
            return .failed(code: .invalidResponse, safeDetail: "Invalid chapter request")
        }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.timeoutInterval = timeoutInterval(deadline: deadline)
        request.setValue(
            "application/json, application/json+chapters, application/chapters+json",
            forHTTPHeaderField: "Accept"
        )
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")

        do {
            let (stream, response) = try await session.bytes(for: request)
            guard let http = response as? HTTPURLResponse,
                  let responseURL = http.url?.absoluteString,
                  let status = UInt16(exactly: http.statusCode)
            else {
                return .failed(code: .invalidResponse, safeDetail: "Non-HTTP chapter response")
            }
            if http.expectedContentLength > 0,
               UInt64(http.expectedContentLength) > maximumResponseBytes {
                return .failed(
                    code: .responseTooLarge,
                    safeDetail: "Chapter response exceeds limit"
                )
            }

            var data = Data()
            data.reserveCapacity(Int(min(maximumResponseBytes, 128 * 1_024)))
            for try await byte in stream {
                try Task.checkCancellation()
                guard UInt64(data.count) < maximumResponseBytes else {
                    return .failed(
                        code: .responseTooLarge,
                        safeDetail: "Chapter response exceeds limit"
                    )
                }
                data.append(byte)
            }
            return .publisherChaptersFetched(
                episodeId: episodeID,
                bytes: data,
                contentType: http.value(forHTTPHeaderField: "Content-Type") ?? "",
                responseUrl: responseURL,
                entityTag: http.value(forHTTPHeaderField: "ETag"),
                lastModified: http.value(forHTTPHeaderField: "Last-Modified"),
                httpStatus: status
            )
        } catch is CancellationError {
            return .cancelled
        } catch let error as URLError {
            return Self.urlFailure(error)
        } catch {
            return .failed(code: .platformFailure, safeDetail: "Chapter request failed")
        }
    }

    private func timeoutInterval(deadline: Date?) -> TimeInterval {
        guard let deadline else { return 30 }
        return max(0.1, min(30, deadline.timeIntervalSinceNow))
    }

    private static func urlFailure(_ error: URLError) -> HostObservation {
        if error.code == .cancelled { return .cancelled }
        let code: HostFailureCode = switch error.code {
        case .notConnectedToInternet, .networkConnectionLost, .internationalRoamingOff:
            .offline
        case .timedOut: .timedOut
        case .userAuthenticationRequired, .userCancelledAuthentication: .permissionDenied
        case .dataLengthExceedsMaximum: .responseTooLarge
        case .badURL, .unsupportedURL, .cannotParseResponse, .badServerResponse:
            .invalidResponse
        default: .platformFailure
        }
        return .failed(code: code, safeDetail: "Chapter transport failed")
    }
}
