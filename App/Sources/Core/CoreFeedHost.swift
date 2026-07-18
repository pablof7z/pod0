import Foundation
import Pod0Core

protocol CoreFeedHosting: Sendable {
    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation
}

/// Executes only the platform networking primitive. Feed parsing,
/// normalization, retry policy, and durable writes remain core decisions.
struct CoreFeedHost: CoreFeedHosting, Sendable {
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        guard maximumResponseBytes > 0,
              let url = URL(string: feedURL),
              let scheme = url.scheme?.lowercased(),
              scheme == "https" || scheme == "http"
        else {
            return .failed(code: .invalidResponse, safeDetail: "Invalid feed request")
        }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.timeoutInterval = timeoutInterval(deadline: deadline)
        request.setValue(
            "application/rss+xml, application/atom+xml, application/xml;q=0.9, */*;q=0.8",
            forHTTPHeaderField: "Accept"
        )
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")
        if let entityTag, !entityTag.isEmpty {
            request.setValue(entityTag, forHTTPHeaderField: "If-None-Match")
        }
        if let lastModified, !lastModified.isEmpty {
            request.setValue(lastModified, forHTTPHeaderField: "If-Modified-Since")
        }

        do {
            let (bytes, response) = try await session.bytes(for: request)
            guard let response = response as? HTTPURLResponse,
                  let responseURL = response.url?.absoluteString
            else {
                return .failed(code: .invalidResponse, safeDetail: "Non-HTTP feed response")
            }
            let responseEntityTag = response.value(forHTTPHeaderField: "ETag")
            let responseLastModified = response.value(forHTTPHeaderField: "Last-Modified")

            if response.statusCode == 304 {
                return .feedNotModified(
                    entityTag: responseEntityTag ?? entityTag,
                    lastModified: responseLastModified ?? lastModified,
                    responseUrl: responseURL
                )
            }
            guard (200...299).contains(response.statusCode) else {
                return Self.httpFailure(statusCode: response.statusCode)
            }
            if response.expectedContentLength > 0,
               UInt64(response.expectedContentLength) > maximumResponseBytes {
                return .failed(code: .responseTooLarge, safeDetail: "Feed response exceeds limit")
            }

            var data = Data()
            data.reserveCapacity(Int(min(maximumResponseBytes, 256 * 1_024)))
            for try await byte in bytes {
                try Task.checkCancellation()
                guard UInt64(data.count) < maximumResponseBytes else {
                    return .failed(
                        code: .responseTooLarge,
                        safeDetail: "Feed response exceeds limit"
                    )
                }
                data.append(byte)
            }
            return .feedBytesFetched(
                bytes: data,
                entityTag: responseEntityTag,
                lastModified: responseLastModified,
                responseUrl: responseURL,
                httpStatus: UInt16(response.statusCode)
            )
        } catch is CancellationError {
            return .cancelled
        } catch let error as URLError {
            return Self.urlFailure(error)
        } catch {
            return .failed(code: .platformFailure, safeDetail: "Feed request failed")
        }
    }

    private func timeoutInterval(deadline: Date?) -> TimeInterval {
        guard let deadline else { return 30 }
        return max(0.1, min(30, deadline.timeIntervalSinceNow))
    }

    private static func httpFailure(statusCode: Int) -> HostObservation {
        let code: HostFailureCode = switch statusCode {
        case 401, 403: .permissionDenied
        case 408, 504: .timedOut
        case 413: .responseTooLarge
        default: .invalidResponse
        }
        return .failed(code: code, safeDetail: "Feed HTTP \(statusCode)")
    }

    private static func urlFailure(_ error: URLError) -> HostObservation {
        if error.code == .cancelled { return .cancelled }
        let code: HostFailureCode = switch error.code {
        case .notConnectedToInternet, .networkConnectionLost, .internationalRoamingOff:
            .offline
        case .timedOut: .timedOut
        case .userAuthenticationRequired, .userCancelledAuthentication: .permissionDenied
        case .badURL, .unsupportedURL, .cannotParseResponse, .badServerResponse: .invalidResponse
        default: .platformFailure
        }
        return .failed(code: code, safeDetail: "Feed transport failed")
    }
}
