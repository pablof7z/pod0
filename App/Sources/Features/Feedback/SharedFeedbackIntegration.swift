import Foundation
import ShakeFeedbackKit

extension ShakeFeedbackConfig {
    static let podcastr = ShakeFeedbackConfig(
        appName: "Pod0",
        clientTag: "podcastr-ios",
        projectATag: "31933:09d48a1a5dbe13404a729634f1d6ba722d40513468dd713c8ea38ca9b7b6f2c7:podcast"
    )
}

/// Host-signer adapter that bridges ShakeFeedbackKit's `ShakeFeedbackSigner`
/// seam to the NMP kernel (D13 — no signing in Swift). The SDK builds the
/// feedback event / NIP-42 AUTH draft and hands it here; we sign it through
/// `nmp_app_sign_event_for_return` (active account) and return the kernel's
/// flat wire event. The SDK's in-package Schnorr signer (`ShakeFeedbackCrypto`)
/// is never reached on this path.
struct PodcastShakeFeedbackSigner: ShakeFeedbackSigner, @unchecked Sendable {
    weak var identity: UserIdentityStore?
    weak var kernel: KernelModel?

    var publicKeyHex: String? {
        get async {
            await MainActor.run { identity?.publicKeyHex }
        }
    }

    func signFeedbackEvent(_ draft: ShakeFeedbackEventDraft) async throws -> ShakeFeedbackEvent {
        // D13: sign through the kernel, never in Swift. The kernel re-stamps
        // `created_at` (D7) and fills `pubkey`/`id`/`sig`; the draft carries
        // only kind/content/tags.
        guard let kernel else { throw ShakeFeedbackError.missingIdentity }
        guard await MainActor.run(body: { identity?.publicKeyHex }) != nil else {
            throw ShakeFeedbackError.missingIdentity
        }
        let unsigned: [String: Any] = [
            "kind": draft.kind,
            "content": draft.content,
            "tags": draft.tags,
            "created_at": draft.createdAt,
        ]
        guard
            let data = try? JSONSerialization.data(withJSONObject: unsigned),
            let unsignedJSON = String(data: data, encoding: .utf8)
        else {
            throw ShakeFeedbackError.invalidEvent("could not serialize the feedback draft")
        }
        let signedJSON = try await kernel.signEventForReturn(
            accountPubkeyHex: "", unsignedJSON: unsignedJSON)
        guard
            let eventData = signedJSON.data(using: .utf8),
            let event = try? JSONDecoder().decode(ShakeFeedbackEvent.self, from: eventData)
        else {
            throw ShakeFeedbackError.invalidEvent("kernel returned an undecodable signed event")
        }
        return event
    }
}
