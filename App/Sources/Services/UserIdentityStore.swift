import Foundation
import Observation
import os.log

/// UI-facing projection of the human identity owned by Pod0's single NMP
/// composition. This object retains no secret and creates no signer transport.
@MainActor
@Observable
final class UserIdentityStore {
    let logger = Logger.app("UserIdentityStore")

    var publicKeyHex: String?
    var loginError: String?

    enum Mode: String, Sendable, Codable {
        case none
        case localKey
        case remoteSigner
    }
    var mode: Mode = .none

    var remoteSignerState: RemoteSignerState = .idle

    var profileDisplayName: String?
    var profileName: String?
    var profileAbout: String?
    var profilePicture: String?

    #if canImport(NMP)
    @ObservationIgnored var nmpComposition: Pod0NMPComposition?
    @ObservationIgnored var nmpLifecycle: Pod0HumanIdentityLifecycle?
    #endif

    var hasIdentity: Bool { publicKeyHex != nil }
    var isRemoteSigner: Bool { mode == .remoteSigner }

    var npub: String? {
        guard let hex = publicKeyHex,
              let bytes = Data(hexString: hex), bytes.count == 32 else { return nil }
        return Bech32.encode(hrp: "npub", data: bytes)
    }

    var npubShort: String? {
        guard let full = npub, full.count > 16 else { return npub }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }

    static let shared = UserIdentityStore()

    func clearPublishedState() {
        publicKeyHex = nil
        mode = .none
        remoteSignerState = .idle
        profileDisplayName = nil
        profileName = nil
        profileAbout = nil
        profilePicture = nil
    }

}

enum RemoteSignerState: Sendable, Equatable {
    case idle
    case connecting
    case reconnecting
    case awaitingAuthorization(URL)
    case connected(String)
    case failed(String)
}

enum UserIdentityError: LocalizedError {
    case noIdentity
    case nmpUnavailable
    case nmpKeyGenerationUnavailable

    var errorDescription: String? {
        switch self {
        case .noIdentity:
            "No identity is available."
        case .nmpUnavailable:
            "The identity engine is unavailable."
        case .nmpKeyGenerationUnavailable:
            "Creating a new identity is unavailable until NMP issue #588 provides secure key generation. Import an nsec or connect a bunker instead."
        }
    }
}
