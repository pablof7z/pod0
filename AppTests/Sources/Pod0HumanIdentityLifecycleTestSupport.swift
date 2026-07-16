import Foundation
import NMP
@testable import Podcastr

struct LifecycleFixture {
    let root: URL
    let composition: Pod0NMPComposition

    func dispose() {
        composition.shutdown()
        try? composition.resetStoreAfterShutdown()
        try? FileManager.default.removeItem(at: root)
    }
}

enum TestIdentityStorageError: Error {
    case injected
}

final class TestLocalAccountCheckpoint: NMPLocalAccountCheckpoint, @unchecked Sendable {
    private let lock = NSLock()
    private var secret: String?
    private var pendingClearFailures = 0
    private var clearAttempts = 0

    var storedSecret: String? { lock.withLock { secret } }
    var clearAttemptCount: Int { lock.withLock { clearAttempts } }

    func loadSecretKey() throws -> String? { lock.withLock { secret } }

    func saveSecretKey(_ secretKey: String) throws {
        lock.withLock { secret = secretKey }
    }

    func clear() throws {
        try lock.withLock {
            clearAttempts += 1
            if pendingClearFailures > 0 {
                pendingClearFailures -= 1
                throw TestIdentityStorageError.injected
            }
            secret = nil
        }
    }

    func failNextClear() {
        lock.withLock { pendingClearFailures += 1 }
    }
}

final class TestIdentityCatalogStorage: Pod0IdentityCatalogStorage, @unchecked Sendable {
    private let lock = NSLock()
    private var catalog: Pod0IdentityCatalog?
    private var pendingSaveFailures = 0

    func load() throws -> Pod0IdentityCatalog? { lock.withLock { catalog } }

    func save(_ catalog: Pod0IdentityCatalog) throws {
        try lock.withLock {
            if pendingSaveFailures > 0 {
                pendingSaveFailures -= 1
                throw TestIdentityStorageError.injected
            }
            self.catalog = catalog
        }
    }

    func clear() throws {
        lock.withLock { catalog = nil }
    }

    func failNextSave() {
        lock.withLock { pendingSaveFailures += 1 }
    }
}

func makeFixture(
    root: URL? = nil,
    checkpoint: TestLocalAccountCheckpoint
) throws -> LifecycleFixture {
    let root = root ?? temporaryRoot("lifecycle")
    let layout = Pod0NMPStoreLayout(rootDirectory: root)
    let configuration = Pod0NMPConfiguration(
        storeURL: layout.storeURL,
        indexerRelays: [],
        operatorRelay: nil,
        fallbackRelays: []
    )
    return LifecycleFixture(
        root: root,
        composition: try Pod0NMPComposition(
            configuration: configuration,
            layout: layout,
            localAccountStore: checkpoint
        )
    )
}

func temporaryRoot(_ name: String) -> URL {
    FileManager.default.temporaryDirectory
        .appendingPathComponent("pod0-identity-\(name)-\(UUID().uuidString)", isDirectory: true)
}

func catalogContaining(_ entry: Pod0IdentityCatalogEntry) -> Pod0IdentityCatalog {
    var catalog = Pod0IdentityCatalog(selectedRole: .human)
    catalog.upsert(entry)
    return catalog
}
