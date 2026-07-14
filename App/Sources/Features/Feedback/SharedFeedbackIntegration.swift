import Foundation
import ShakeFeedbackKit

extension ShakeFeedbackConfig {
    static let podcastr = ShakeFeedbackConfig(
        appName: "Podcastr",
        clientTag: "podcastr-ios",
        projectATag: "31933:09d48a1a5dbe13404a729634f1d6ba722d40513468dd713c8ea38ca9b7b6f2c7:podcast"
    )
}

struct PodcastShakeFeedbackSigner: ShakeFeedbackSigner, @unchecked Sendable {
    weak var identity: UserIdentityStore?

    var publicKeyHex: String? {
        get async {
            await MainActor.run { identity?.publicKeyHex }
        }
    }

    func signFeedbackEvent(_ draft: ShakeFeedbackEventDraft) async throws -> ShakeFeedbackEvent {
        guard let signer = await MainActor.run(body: { identity?.signer }) else {
            throw ShakeFeedbackError.missingIdentity
        }
        let event = try await signer.sign(NostrEventDraft(
            kind: draft.kind,
            content: draft.content,
            tags: draft.tags,
            createdAt: draft.createdAt
        ))
        return ShakeFeedbackEvent(
            id: event.id,
            pubkey: event.pubkey,
            createdAt: event.created_at,
            kind: event.kind,
            tags: event.tags,
            content: event.content,
            sig: event.sig
        )
    }
}
