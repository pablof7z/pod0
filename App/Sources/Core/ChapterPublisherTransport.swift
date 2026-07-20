import Foundation

struct ChapterPublisherTransportResponse: Equatable, Sendable {
    let bytes: Data
    let responseURL: String
    let contentType: String
    let entityTag: String?
    let lastModified: String?
    let httpStatus: UInt16
}

protocol ChapterPublisherTransporting: Sendable {
    func fetch(
        _ request: PublisherChapterCapabilityRequest,
        maximumResponseBytes: UInt64
    ) async -> Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure>
}

/// Native URLSession executor. It returns transport facts only; Rust parses
/// Podcasting 2.0 content and decides whether the observation is usable.
struct LiveChapterPublisherTransport: ChapterPublisherTransporting, Sendable {
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func fetch(
        _ request: PublisherChapterCapabilityRequest,
        maximumResponseBytes: UInt64
    ) async -> Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure> {
        guard maximumResponseBytes > 0,
              let url = URL(string: request.sourceURL),
              let scheme = url.scheme?.lowercased(),
              scheme == "https" || scheme == "http"
        else {
            return .failure(.invalidRequest("Invalid publisher chapter URL"))
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "GET"
        urlRequest.timeoutInterval = timeout(deadline: request.deadlineAt?.date)
        urlRequest.setValue(
            "application/json, application/json+chapters, application/chapters+json",
            forHTTPHeaderField: "Accept"
        )
        urlRequest.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")

        do {
            let (stream, response) = try await session.bytes(for: urlRequest)
            guard let http = response as? HTTPURLResponse,
                  let responseURL = http.url?.absoluteString,
                  let status = UInt16(exactly: http.statusCode)
            else {
                return .failure(.invalidMetadata("Non-HTTP publisher response"))
            }
            guard (200...299).contains(http.statusCode) else {
                return .failure(Self.httpFailure(status))
            }
            if http.expectedContentLength > 0,
               UInt64(http.expectedContentLength) > maximumResponseBytes {
                return .failure(.responseTooLarge("Publisher response exceeds core limit"))
            }

            var data = Data()
            data.reserveCapacity(Int(min(maximumResponseBytes, 128 * 1_024)))
            for try await byte in stream {
                try Task.checkCancellation()
                guard UInt64(data.count) < maximumResponseBytes else {
                    return .failure(.responseTooLarge("Publisher response exceeds core limit"))
                }
                data.append(byte)
            }
            return .success(ChapterPublisherTransportResponse(
                bytes: data,
                responseURL: responseURL,
                contentType: http.value(forHTTPHeaderField: "Content-Type") ?? "",
                entityTag: http.value(forHTTPHeaderField: "ETag"),
                lastModified: http.value(forHTTPHeaderField: "Last-Modified"),
                httpStatus: status
            ))
        } catch is CancellationError {
            return .failure(.cancelled)
        } catch let error as URLError {
            return .failure(Self.urlFailure(error))
        } catch {
            return .failure(ChapterCapabilityFailure(
                code: .transport,
                httpStatus: nil,
                safeDetail: "Publisher transport failed"
            ))
        }
    }

    private func timeout(deadline: Date?) -> TimeInterval {
        guard let deadline else { return 30 }
        return max(0.1, min(30, deadline.timeIntervalSinceNow))
    }

    private static func httpFailure(_ status: UInt16) -> ChapterCapabilityFailure {
        let code: ChapterCapabilityFailureCode = switch status {
        case 401, 403: .authentication
        case 413: .responseTooLarge
        default: .transport
        }
        return ChapterCapabilityFailure(
            code: code,
            httpStatus: status,
            safeDetail: "Publisher HTTP \(status)"
        )
    }

    private static func urlFailure(_ error: URLError) -> ChapterCapabilityFailure {
        if error.code == .cancelled { return .cancelled }
        let code: ChapterCapabilityFailureCode = switch error.code {
        case .userAuthenticationRequired, .userCancelledAuthentication: .authentication
        case .dataLengthExceedsMaximum: .responseTooLarge
        default: .transport
        }
        return ChapterCapabilityFailure(
            code: code,
            httpStatus: nil,
            safeDetail: "Publisher transport failed"
        )
    }
}
