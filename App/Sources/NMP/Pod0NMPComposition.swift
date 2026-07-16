import Foundation

#if canImport(NMP)
import NMP
#endif

enum Pod0NMPCompositionError: Error, Equatable {
    case storeAlreadyOwned(String)
    case engineStillRunning
    case engineShutdown
    case nmpUnavailable
}

enum Pod0NMPLifecycleOperation: Sendable, Equatable {
    /// Changes the active signing identity without changing canonical rows or
    /// retargeting already accepted obligations.
    case switchAccount(expectedPublicKey: String?)
    /// Detaches live signing capability while retaining public cache and
    /// parked durable obligations.
    case cachePreservingSignOut
    /// Stops the engine and deletes only its canonical store. Keychain and
    /// product state are deliberately outside this operation.
    case resetNostrData
    /// A trust-domain boundary: stop and reset the NMP store before another
    /// mutually untrusted person uses the installation.
    case resetForMutuallyUntrustedUser
}

/// Narrow injection seam for product slices that need the one shared engine.
/// Test code should inject the product slice's own repository protocol rather
/// than opening a second NMP store.
protocol Pod0NMPEngineAccess: Sendable {
    var configuration: Pod0NMPConfiguration { get }

    #if canImport(NMP)
    var engine: NMPEngine { get }
    #endif
}

/// Process-local ownership guard. NMP also rejects a live reset itself; this
/// guard gives Pod0 a deterministic error before construction and makes the
/// one-owner composition rule directly testable without opening a real store.
final class Pod0NMPStoreLease: @unchecked Sendable {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var claimedPaths: Set<String> = []

    let path: String
    private let stateLock = NSLock()
    private var released = false

    static func acquire(path: String) throws -> Pod0NMPStoreLease {
        let canonical = URL(fileURLWithPath: path).standardizedFileURL.path
        try lock.withLock {
            guard claimedPaths.insert(canonical).inserted else {
                throw Pod0NMPCompositionError.storeAlreadyOwned(canonical)
            }
        }
        return Pod0NMPStoreLease(path: canonical)
    }

    private init(path: String) {
        self.path = path
    }

    func release() {
        let shouldRelease = stateLock.withLock { () -> Bool in
            guard !released else { return false }
            released = true
            return true
        }
        guard shouldRelease else { return }
        Self.lock.withLock { _ = Self.claimedPaths.remove(path) }
    }

    deinit { release() }
}

#if canImport(NMP)
/// The sole long-lived owner of Pod0's NMP engine and canonical store.
/// Construction is dependency-injected from `AppMain`; there is intentionally
/// no hidden singleton and no scene-phase, reconnect, replay, or polling hook.
final class Pod0NMPComposition: Pod0NMPEngineAccess, @unchecked Sendable {
    let configuration: Pod0NMPConfiguration
    let layout: Pod0NMPStoreLayout
    let engine: NMPEngine

    private let lease: Pod0NMPStoreLease
    private let stateLock = NSLock()
    private var shutdownState = false
    private var staged: Pod0NMPConfiguration?

    var stagedConfiguration: Pod0NMPConfiguration? {
        stateLock.withLock { staged }
    }

    init(
        configuration: Pod0NMPConfiguration,
        layout: Pod0NMPStoreLayout,
        localAccountStore: (any NMPLocalAccountCheckpoint)? = nil,
        fileManager: FileManager = .default
    ) throws {
        try layout.prepare(fileManager: fileManager)
        let lease = try Pod0NMPStoreLease.acquire(path: configuration.storePath)
        do {
            let nmpConfiguration = NMPConfig(
                storePath: configuration.storePath,
                indexerRelays: configuration.indexerRelays,
                appRelays: configuration.appRelays,
                fallbackRelays: configuration.fallbackRelays,
                allowedLocalRelayHosts: configuration.allowedLocalRelayHosts,
                maxRelays: configuration.limits.maxRelays,
                maxNativeTasks: configuration.limits.maxNativeTasks,
                maxAuthCapabilities: configuration.limits.maxAuthCapabilities
            )
            engine = try NMPEngine(
                config: nmpConfiguration,
                localAccountStore: localAccountStore
            )
        } catch {
            lease.release()
            throw error
        }
        self.configuration = configuration
        self.layout = layout
        self.lease = lease
    }

    /// Records an immutable next-construction policy. It does not create,
    /// overlap, or hot-retarget an engine.
    func stageOperatorRelay(_ relay: String?) throws {
        try stateLock.withLock {
            guard !shutdownState else { throw Pod0NMPCompositionError.engineShutdown }
            staged = configuration.stagingOperatorRelay(relay)
        }
    }

    func shutdown() {
        let shouldShutdown = stateLock.withLock { () -> Bool in
            guard !shutdownState else { return false }
            shutdownState = true
            return true
        }
        guard shouldShutdown else { return }
        engine.shutdown()
        lease.release()
    }

    /// Reset is structurally separate from sign-out and refuses while this
    /// composition is live. NMP's own process-local live-store guard remains
    /// the final authority at the deletion door.
    func resetStoreAfterShutdown() throws {
        guard stateLock.withLock({ shutdownState }) else {
            throw Pod0NMPCompositionError.engineStillRunning
        }
        try NMPEngine.resetPersistentStore(at: configuration.storePath)
    }

    deinit { shutdown() }
}
#endif
