import Foundation
import Pod0Core

/// Literal URLSession capability for exact Rust-authored library requests.
/// It performs no endpoint selection, parsing, matching, retry, or persistence.
struct CoreLibraryNetworkHost: Sendable {
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func fetch(
        workflowCommandID: CommandId,
        step: LibraryNetworkStep,
        url rawURL: String,
        accept: String,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        guard maximumResponseBytes > 0,
              let url = URL(string: rawURL),
              ["http", "https"].contains(url.scheme?.lowercased() ?? "")
        else {
            return .failed(code: .invalidResponse, safeDetail: "Invalid library request")
        }
        var request = URLRequest(url: url)
        request.timeoutInterval = max(0.1, min(30, deadline?.timeIntervalSinceNow ?? 30))
        request.setValue(accept, forHTTPHeaderField: "Accept")
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")
        do {
            let (stream, response) = try await session.bytes(for: request)
            guard let http = response as? HTTPURLResponse,
                  let responseURL = http.url?.absoluteString
            else {
                return .failed(code: .invalidResponse, safeDetail: "Non-HTTP library response")
            }
            guard (200...299).contains(http.statusCode) else {
                return .failed(code: Self.failure(http.statusCode), safeDetail: "Library HTTP response")
            }
            if http.expectedContentLength > 0,
               UInt64(http.expectedContentLength) > maximumResponseBytes {
                return .failed(code: .responseTooLarge, safeDetail: "Library response exceeds limit")
            }
            var data = Data()
            data.reserveCapacity(Int(min(maximumResponseBytes, 256 * 1_024)))
            for try await byte in stream {
                try Task.checkCancellation()
                guard UInt64(data.count) < maximumResponseBytes else {
                    return .failed(code: .responseTooLarge, safeDetail: "Library response exceeds limit")
                }
                data.append(byte)
            }
            return .libraryDocumentFetched(
                workflowCommandId: workflowCommandID,
                step: step,
                bytes: data,
                responseUrl: responseURL,
                mimeType: http.mimeType,
                httpStatus: UInt16(http.statusCode)
            )
        } catch is CancellationError {
            return .cancelled
        } catch let error as URLError {
            let code: HostFailureCode = switch error.code {
            case .notConnectedToInternet, .networkConnectionLost: .offline
            case .timedOut: .timedOut
            default: .platformFailure
            }
            return .failed(code: code, safeDetail: "Library transport failed")
        } catch {
            return .failed(code: .platformFailure, safeDetail: "Library request failed")
        }
    }

    private static func failure(_ status: Int) -> HostFailureCode {
        switch status {
        case 401, 403: .permissionDenied
        case 408, 504: .timedOut
        case 413: .responseTooLarge
        default: .invalidResponse
        }
    }
}
