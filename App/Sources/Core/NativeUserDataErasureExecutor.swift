import Foundation
import Pod0Core

enum NativeUserDataErasureError: Error {
    case invalidAction
    case incompleteAction
}

@MainActor
enum NativeUserDataErasureExecutor {
    static func finish(
        _ initial: UserDataErasureResult,
        locations: UserDataErasureLocations
    ) async throws -> CommandId {
        var progress = initial
        while true {
            switch progress {
            case .complete(let freshStoreID):
                return freshStoreID
            case .awaitingNativeActions(let actions):
                guard let action = actions.first else {
                    throw NativeUserDataErasureError.invalidAction
                }
                do {
                    try await execute(action)
                    progress = try recordNativeErasureObservation(
                        locations: locations,
                        actionId: action.actionId,
                        observedAttempt: action.attempt,
                        succeeded: true
                    )
                } catch {
                    _ = try? recordNativeErasureObservation(
                        locations: locations,
                        actionId: action.actionId,
                        observedAttempt: action.attempt,
                        succeeded: false
                    )
                    throw error
                }
            }
        }
    }

    private static func execute(_ action: NativeErasureAction) async throws {
        switch action.kind {
        case .agentConversationPointer:
            guard action.identifier == "pod0.agent.lastConversationID.v1" else {
                throw NativeUserDataErasureError.invalidAction
            }
            try erase(
                key: action.identifier,
                from: .standard
            )
        case .spotlightIndex:
            guard action.identifier == SpotlightIndexer.erasureIdentifier else {
                throw NativeUserDataErasureError.invalidAction
            }
            try await SpotlightIndexer.eraseAuthorizedDomains()
        case .nowPlayingProjection:
            guard action.identifier == "group.com.podcastr.app/now-playing-snapshot.v1",
                  let defaults = UserDefaults(suiteName: NowPlayingSnapshotStore.appGroupID)
            else {
                throw NativeUserDataErasureError.invalidAction
            }
            try erase(key: NowPlayingSnapshotStore.defaultsKey, from: defaults)
        default:
            throw NativeUserDataErasureError.invalidAction
        }
    }

    private static func erase(key: String, from defaults: UserDefaults) throws {
        defaults.removeObject(forKey: key)
        defaults.synchronize()
        guard defaults.object(forKey: key) == nil else {
            throw NativeUserDataErasureError.incompleteAction
        }
    }
}
