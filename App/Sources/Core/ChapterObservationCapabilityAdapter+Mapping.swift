import Foundation
import Pod0Core

extension ChapterObservationCapabilityAdapter {
    static func preflight(
        _ request: ChapterCapabilityRequest,
        limits: ChapterObservationLimits
    ) -> ChapterCapabilityFailure? {
        switch request {
        case .agent(let value):
            guard value.items.count <= Int(limits.agentItems) else {
                return .responseTooLarge("Agent chapter items exceed core limit")
            }
        }
        return nil
    }
}
