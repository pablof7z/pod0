import Foundation

struct Pod0LegacyIdentityMigration {
    struct RemoteMetadata: Codable, Sendable, Equatable {
        let bunkerPubkeyHex: String
        let relays: [String]
        let secret: String?
        let permissions: [String]
        let userPubkeyHex: String
    }

    private let bundleIdentifier: String

    init(bundleIdentifier: String = Bundle.main.bundleIdentifier ?? "Podcastr") {
        self.bundleIdentifier = bundleIdentifier
    }

    func migratedCatalog(
        existing: Pod0IdentityCatalog?,
        agentExpectedPublicKey: String?,
        now: Date = Date()
    ) throws -> Pod0IdentityCatalog {
        if let existing { return existing }
        var catalog = Pod0IdentityCatalog()
        if let human = try legacyHumanEntry(now: now) {
            catalog.upsert(human)
            try catalog.select(.human)
        }
        if let agentExpectedPublicKey, !agentExpectedPublicKey.isEmpty {
            catalog.upsert(Pod0IdentityCatalogEntry(
                role: .agentPodcast,
                label: "Podcast agent",
                origin: .legacyAgentKey,
                expectedPublicKey: agentExpectedPublicKey,
                capability: .reservedForLaterMilestone(secretReference: "legacy-agent-private-key"),
                createdAt: now
            ))
        }
        return catalog
    }

    private func legacyHumanEntry(now: Date) throws -> Pod0IdentityCatalogEntry? {
        let localService = "\(bundleIdentifier).user-identity"
        if let secret = try KeychainStore.readString(service: localService, account: "user-private-key-hex"),
           !secret.isEmpty {
            let pair = try NostrKeyPair(privateKeyHex: secret)
            let originValue = try KeychainStore.readString(
                service: localService,
                account: "user-private-key-origin"
            )
            return Pod0IdentityCatalogEntry(
                role: .human,
                label: "Personal identity",
                origin: originValue == "generated" ? .generatedLocally : .importedNsec,
                expectedPublicKey: pair.publicKeyHex,
                capability: .localKey(secretReference: "legacy-human-private-key"),
                createdAt: now
            )
        }

        let metaService = "\(bundleIdentifier).nip46-meta"
        guard let json = try KeychainStore.readString(service: metaService, account: "connection"),
              let data = json.data(using: .utf8) else { return nil }
        let metadata = try JSONDecoder().decode(RemoteMetadata.self, from: data)

        // A legacy secret proves this was bunker-origin and gives NMP a
        // restartable descriptor. Without it, the old schema cannot
        // distinguish a secretless bunker from a client-initiated session;
        // fail closed under the #571 blocker instead of guessing.
        let capability: Pod0IdentityCapability
        let origin: Pod0IdentityOrigin
        if metadata.secret != nil {
            capability = .nip46Bunker(uri: Self.bunkerURI(metadata))
            origin = .bunker
        } else {
            capability = .nip46ClientInitiated(relays: metadata.relays)
            origin = .clientInitiatedNostrConnect
        }
        return Pod0IdentityCatalogEntry(
            role: .human,
            label: "Personal remote signer",
            origin: origin,
            expectedPublicKey: metadata.userPubkeyHex,
            capability: capability,
            createdAt: now
        )
    }

    private static func bunkerURI(_ metadata: RemoteMetadata) -> String {
        var components = URLComponents()
        components.scheme = "bunker"
        components.host = metadata.bunkerPubkeyHex
        var query = metadata.relays.map { URLQueryItem(name: "relay", value: $0) }
        if let secret = metadata.secret { query.append(URLQueryItem(name: "secret", value: secret)) }
        if !metadata.permissions.isEmpty {
            query.append(URLQueryItem(name: "perms", value: metadata.permissions.joined(separator: ",")))
        }
        components.queryItems = query
        return components.string ?? "bunker://\(metadata.bunkerPubkeyHex)"
    }
}

