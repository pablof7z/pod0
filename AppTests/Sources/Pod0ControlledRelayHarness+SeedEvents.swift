#if targetEnvironment(simulator)
import Foundation
@testable import Podcastr

extension Pod0ControlledRelayHarness {
    func seed(_ event: SignedNostrEvent) {
        lock.withLock {
            seededEvents.append([
                "id": event.id,
                "pubkey": event.pubkey,
                "created_at": event.created_at,
                "kind": event.kind,
                "tags": event.tags,
                "content": event.content,
                "sig": event.sig,
            ])
        }
    }

    static func event(_ event: [String: Any], matches filter: [String: Any]) -> Bool {
        if let kinds = filter["kinds"] as? [Int],
           let kind = event["kind"] as? Int,
           !kinds.contains(kind) { return false }
        if let authors = filter["authors"] as? [String],
           let pubkey = event["pubkey"] as? String,
           !authors.contains(where: { pubkey.hasPrefix($0) }) { return false }
        return true
    }
}
#endif
