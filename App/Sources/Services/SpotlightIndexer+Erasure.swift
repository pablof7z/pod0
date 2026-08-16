@preconcurrency import CoreSpotlight
import Foundation

extension SpotlightIndexer {
    static let erasureIdentifier = "pod0.search.notes,memories,subscriptions,episodes"

    static func eraseAuthorizedDomains() async throws {
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            CSSearchableIndex.default().deleteSearchableItems(
                withDomainIdentifiers: Domain.allCases.map(\.rawValue)
            ) { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            }
        }
    }
}
