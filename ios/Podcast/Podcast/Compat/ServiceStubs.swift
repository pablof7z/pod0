// Compat shim — service-layer stubs referenced by migrated views.
//
// Every type in this file replaces a real implementation under
// `App/Sources/Services/` that ships with the legacy app. For M1.E we only
// need enough surface to compile the migrated Identity / Onboarding / Agent
// views. Every entry point either no-ops or throws.

import Foundation
import Observation
import SwiftUI

// MARK: - Signer marker protocol

/// Marker protocol — `NostrSigner` is referenced as a type by
/// `BlossomUploading.upload(data:contentType:signer:)` and historically by
/// the legacy `UserIdentityStore.signer` accessor. No methods are called on
/// it in the migrated views, so an empty marker protocol is sufficient.
///
/// Declared `Sendable` so view-side code can pass it across a `Task`
/// boundary into `BlossomUploading.upload`.
///
/// Moved here (from the deleted `UserIdentityStoreCompat.swift`) as part of
/// PR 16 so the M10 Blossom cluster stays self-contained.
protocol NostrSigner: AnyObject, Sendable {}

// MARK: - Compat error

/// Shared error type for the remaining `Compat/` stubs (Blossom upload,
/// `NostrKeyPair`, BYOK connect, etc.). Surfaces a "not yet implemented"
/// message that calling views can display verbatim.
enum CompatError: LocalizedError {
    case notImplemented(String)

    var errorDescription: String? {
        switch self {
        case .notImplemented(let symbol):
            return "\(symbol) is not yet implemented in the compat shim."
        }
    }
}

// MARK: - Blossom upload (M10 stub)

/// Image-upload protocol. M1.E compat — replaced when the Blossom Capability
/// lands in M10. Declared `Sendable` so view-side code can hold a default
/// instance and pass it into a `Task`.
protocol BlossomUploading: AnyObject, Sendable {
    func upload(data: Data, contentType: String, signer: any NostrSigner) async throws -> URL
}

/// Default implementation — fails immediately so the calling view surfaces
/// an "upload failed" banner without crashing.
final class BlossomUploader: BlossomUploading {
    func upload(data: Data, contentType: String, signer: any NostrSigner) async throws -> URL {
        throw CompatError.notImplemented("BlossomUploader.upload")
    }
}

// MARK: - BYOK connect (M1 stub)

/// Errors surfaced by the BYOK pairing flow. Mirrors the legacy enum so the
/// view-side switch arms compile.
enum BYOKConnectError: LocalizedError {
    case cancelled
    case noProviderKeysReturned
    case notImplemented

    var errorDescription: String? {
        switch self {
        case .cancelled: return "Connection cancelled."
        case .noProviderKeysReturned: return "No provider keys were returned."
        case .notImplemented: return "BYOK connect is not yet wired in the M1.E compat shim."
        }
    }
}

/// Compat shim for the legacy BYOK pairing service. The migrated onboarding
/// view stores an instance in `@State` and awaits `connectPodcastProviders()`.
/// Replaced when the signer-broker / BYOK Capability lands.
@MainActor
@Observable
final class BYOKConnectService {
    /// Returns the imported credential tokens. Compat: throws so the calling
    /// view surfaces an error banner.
    func connectPodcastProviders() async throws -> [BYOKToken] {
        throw BYOKConnectError.notImplemented
    }
}

/// Compat token returned by `BYOKConnectService`. Real shape lands with the
/// BYOK Capability.
struct BYOKToken: Hashable, Sendable {
    var provider: String
    var keyID: String
    var keyLabel: String
}

/// Compat helper — mirrors the legacy `PodcastBYOKCredentialImporter.apply`
/// signature used by `OnboardingView+Handlers.handleBYOKConnect`. Returns the
/// provider strings the caller treats as "imported".
enum PodcastBYOKCredentialImporter {
    @discardableResult
    static func apply(_ tokens: [BYOKToken], to settings: inout Settings) throws -> [String] {
        // Compat: no-op import, returns empty so the caller's "noProviderKeysReturned"
        // guard fires and surfaces an error banner instead of silently advancing.
        []
    }
}

// MARK: - OpenRouter credential store (M3 stub)

/// Compat shim — replaced when LLM provider credential Capability lands.
enum OpenRouterCredentialStore {
    static func saveAPIKey(_ key: String) throws {
        throw CompatError.notImplemented("OpenRouterCredentialStore.saveAPIKey")
    }
}

// MARK: - Subscription service (M2 stub)

/// Compat shim — replaced when the subscription projection lands.
@MainActor
struct SubscriptionService {
    let store: AppStateStore

    enum AddError: LocalizedError {
        case notImplemented
        case alreadySubscribed
        case transport(String)

        var errorDescription: String? {
            switch self {
            case .notImplemented:
                return "Adding subscriptions is not yet wired in the M1.E compat shim."
            case .alreadySubscribed:
                return "You are already subscribed to this podcast."
            case .transport(let msg):
                return msg
            }
        }
    }

    init(store: AppStateStore) { self.store = store }
    init(store: KernelModel) { self.store = AppStateStore() }

    func addSubscription(feedURLString: String) async throws -> Podcast {
        throw AddError.notImplemented
    }

    func refresh(_ podcast: Podcast) async {}
    func fetchForAdoption(opmlEntry: Podcast) async throws -> SubscriptionImportPayload? { nil }
}

// MARK: - LiquidGlassSegmentedPicker (compat stub)

/// Compat shim — the real Liquid Glass segmented picker is a custom Design
/// component. For M1.E we render a plain SwiftUI Picker so Library views compile.
struct LiquidGlassSegmentedPicker<V: Hashable>: View {
    let label: String
    @Binding var selection: V
    let segments: [(V, String)]

    init(_ label: String, selection: Binding<V>, segments: [(V, String)]) {
        self.label = label
        self._selection = selection
        self.segments = segments
    }

    var body: some View {
        Picker(label, selection: $selection) {
            ForEach(segments, id: \.0.hashValue) { (value, title) in
                Text(title).tag(value)
            }
        }
        .pickerStyle(.segmented)
    }
}

// MARK: - Nostr credential store (M1 stub)

/// Compat shim — replaced when nmp-signer-broker exposes a key-storage
/// surface that mirrors the legacy keychain helpers.
///
/// Backed by an `@MainActor`-isolated storage box so the static helpers
/// satisfy Swift 6 strict concurrency. Functional storage replaces this when
/// the BYOK Capability lands.
enum NostrCredentialStore {
    @MainActor private final class Storage {
        static let shared = Storage()
        var hex: String?
    }

    @MainActor
    static func savePrivateKey(_ hex: String) throws {
        Storage.shared.hex = hex
    }

    @MainActor
    static func hasPrivateKey() -> Bool {
        Storage.shared.hex != nil
    }
}

// MARK: - Nostr key pair (M1 stub)

/// Compat shim — replaced when nmp-keys exposes a Swift-facing key pair type
/// through the FFI. For M1.E we only need a struct that the migrated views
/// can construct without crashing.
struct NostrKeyPair {
    let privateKeyHex: String
    let publicKeyHex: String

    /// Throwing init used by view-side imports — the legacy NostrKeyPair init
    /// is throwing because of bech32 / secp256k1 validation. Compat shim
    /// performs a minimal non-empty check so the caller's catch arm is still
    /// exercised by an empty input.
    init(privateKeyHex hex: String) throws {
        guard !hex.isEmpty else { throw CompatError.notImplemented("NostrKeyPair init") }
        self.privateKeyHex = hex
        // Placeholder: pubkey derivation needs secp256k1. Until nmp-keys is
        // wired, we surface the private hex so UI doesn't render empty.
        self.publicKeyHex = hex
    }

    init(nsec: String) throws {
        // Compat: parsing nsec needs Bech32 decode. M1.E stub treats the
        // input as opaque hex so the calling view's "invalid key" path is
        // not entered. Replaced when nmp-keys lands.
        guard !nsec.isEmpty else { throw CompatError.notImplemented("NostrKeyPair(nsec:)") }
        self.privateKeyHex = nsec
        self.publicKeyHex = nsec
    }

    static func generate() throws -> NostrKeyPair {
        // Compat: emit a random 32-byte hex so the agent identity view can
        // proceed past key generation. Not a real secp256k1 keypair.
        var bytes = [UInt8](repeating: 0, count: 32)
        let result = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        guard result == errSecSuccess else {
            throw CompatError.notImplemented("NostrKeyPair.generate (SecRandom)")
        }
        let hex = bytes.map { String(format: "%02x", $0) }.joined()
        return try NostrKeyPair(privateKeyHex: hex)
    }
}

// MARK: - NIP-46 connect card (compat stub)

/// Compat shim — the real card lives in
/// `App/Sources/Features/Feedback/Nip46ConnectCard.swift` and renders the
/// full pairing UX. For M1.E we expose the same initializer and emit a
/// placeholder so `RemoteSignerView` compiles. Replaced when the NIP-46
/// pairing UI is migrated.
struct Nip46ConnectCard: View {
    enum Presentation { case card, primary }

    @Binding var bunkerInput: String
    @Binding var isConnectingRemote: Bool
    let connect: () async -> Void
    let disconnect: () async -> Void
    var presentation: Presentation = .card

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            TextField("bunker://…", text: $bunkerInput)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .textFieldStyle(.roundedBorder)
            HStack {
                Button("Connect") {
                    Task { await connect() }
                }
                .disabled(isConnectingRemote || bunkerInput.isBlank)
                if isConnectingRemote { ProgressView() }
                Spacer()
                Button("Disconnect") {
                    Task { await disconnect() }
                }
                .foregroundStyle(.red)
            }
            Text("NIP-46 pairing UI lands in a follow-up milestone.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Agent connection settings view (compat stub)

/// Compat shim — the real connection-settings sheet lives in the legacy app.
/// For M1.E we render a placeholder so `AgentIdentityView` can present it
/// without crashing.
struct AgentConnectionSettingsView: View {
    @Binding var relayURL: String
    let hasPrivateKey: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("Relay") {
                    TextField("wss://…", text: $relayURL)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                }
                Section {
                    Text("Connection settings UI lands in a follow-up milestone.")
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Connection")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
