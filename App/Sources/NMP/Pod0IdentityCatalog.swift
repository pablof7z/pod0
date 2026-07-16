import Foundation

enum Pod0IdentityRole: String, Sendable, Codable, CaseIterable {
    case human
    case agentPodcast
}

enum Pod0IdentityOrigin: String, Sendable, Codable {
    case generatedLocally
    case importedNsec
    case bunker
    case clientInitiatedNostrConnect
}

enum Pod0IdentityCapability: Sendable, Codable, Equatable {
    case localKey(secretReference: String)
    case nip46Bunker(uri: String)
    case nip46ClientInitiated(relays: [String])
    case reservedForLaterMilestone
}

struct Pod0IdentityCatalogEntry: Sendable, Codable, Equatable, Identifiable {
    var id: Pod0IdentityRole { role }
    let role: Pod0IdentityRole
    var label: String
    let origin: Pod0IdentityOrigin
    let expectedPublicKey: String
    let capability: Pod0IdentityCapability
    let createdAt: Date
}

struct Pod0IdentityCatalog: Sendable, Codable, Equatable {
    static let schemaVersion = 1

    let schemaVersion: Int
    var selectedRole: Pod0IdentityRole?
    var entries: [Pod0IdentityCatalogEntry]

    init(selectedRole: Pod0IdentityRole? = nil, entries: [Pod0IdentityCatalogEntry] = []) {
        schemaVersion = Self.schemaVersion
        self.selectedRole = selectedRole
        self.entries = entries
    }

    func entry(for role: Pod0IdentityRole) -> Pod0IdentityCatalogEntry? {
        entries.first { $0.role == role }
    }

    mutating func upsert(_ entry: Pod0IdentityCatalogEntry) {
        entries.removeAll { $0.role == entry.role }
        entries.append(entry)
        entries.sort { $0.role.rawValue < $1.role.rawValue }
    }

    mutating func select(_ role: Pod0IdentityRole?) throws {
        if let role, entry(for: role) == nil {
            throw Pod0IdentityCatalogError.roleNotFound(role)
        }
        selectedRole = role
    }
}
enum Pod0IdentityCatalogError: Error, Equatable {
    case unsupportedSchema(Int)
    case roleNotFound(Pod0IdentityRole)
    case corruptCatalog
}

protocol Pod0IdentityCatalogStorage: Sendable {
    func load() throws -> Pod0IdentityCatalog?
    func save(_ catalog: Pod0IdentityCatalog) throws
    func clear() throws
}

/// Pod0 owns multi-role inventory and consent. This Keychain value stores
/// labels, origins, expected pubkeys, and encrypted reconnect metadata; NMP's
/// one-account checkpoint is intentionally not used as a multi-account vault.
struct KeychainPod0IdentityCatalogStorage: Pod0IdentityCatalogStorage {
    private let service: String
    private let account = "catalog-v1"

    init(bundleIdentifier: String = Bundle.main.bundleIdentifier ?? "Podcastr") {
        service = "\(bundleIdentifier).nmp-identity-catalog"
    }

    func load() throws -> Pod0IdentityCatalog? {
        guard let json = try KeychainStore.readString(service: service, account: account),
              let data = json.data(using: .utf8) else { return nil }
        let catalog = try JSONDecoder().decode(Pod0IdentityCatalog.self, from: data)
        guard catalog.schemaVersion == Pod0IdentityCatalog.schemaVersion else {
            throw Pod0IdentityCatalogError.unsupportedSchema(catalog.schemaVersion)
        }
        return catalog
    }

    func save(_ catalog: Pod0IdentityCatalog) throws {
        guard catalog.schemaVersion == Pod0IdentityCatalog.schemaVersion else {
            throw Pod0IdentityCatalogError.unsupportedSchema(catalog.schemaVersion)
        }
        let data = try JSONEncoder().encode(catalog)
        guard let json = String(data: data, encoding: .utf8) else {
            throw Pod0IdentityCatalogError.corruptCatalog
        }
        try KeychainStore.saveString(json, service: service, account: account)
    }

    func clear() throws {
        try KeychainStore.deleteString(service: service, account: account)
    }
}
