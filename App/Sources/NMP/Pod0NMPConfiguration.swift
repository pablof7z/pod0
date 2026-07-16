import Foundation

/// The exact upstream source revision whose generated Swift and binary
/// artifacts must be present in this checkout. Bootstrap may replace this
/// value from repository-contained pin metadata, but it must never resolve a
/// branch name at runtime.
enum Pod0NMPBuild {
    static let testedRevision = "867aecfd83aad47a3ec31ff07f0c564505da0eef"
}

/// Construction-time policy for Pod0's one NMP trust domain.
///
/// Relay lanes and resource ceilings are immutable for the lifetime of an
/// engine. `nostrPublicRelays` is intentionally not accepted here: it is
/// legacy discovered state without trustworthy provenance, not operator
/// configuration.
struct Pod0NMPConfiguration: Sendable, Codable, Hashable {
    struct Limits: Sendable, Codable, Hashable {
        let maxRelays: UInt32
        let maxNativeTasks: UInt32
        let maxAuthCapabilities: UInt32

        static let appDefault = Limits(
            maxRelays: 12,
            maxNativeTasks: 16,
            maxAuthCapabilities: 8
        )

        init(maxRelays: UInt32, maxNativeTasks: UInt32, maxAuthCapabilities: UInt32) {
            precondition(maxRelays > 0, "NMP relay capacity must be finite and non-zero")
            precondition(maxNativeTasks > 0, "NMP native-task capacity must be finite and non-zero")
            precondition(maxAuthCapabilities > 0, "NMP auth capacity must be finite and non-zero")
            self.maxRelays = maxRelays
            self.maxNativeTasks = maxNativeTasks
            self.maxAuthCapabilities = maxAuthCapabilities
        }
    }

    let storePath: String
    let indexerRelays: [String]
    let appRelays: [String]
    let fallbackRelays: [String]
    let allowedLocalRelayHosts: [String]
    let limits: Limits
    let nmpRevision: String

    init(
        storeURL: URL,
        indexerRelays: [String],
        operatorRelay: String?,
        fallbackRelays: [String],
        allowedLocalRelayHosts: [String] = [],
        limits: Limits = .appDefault,
        nmpRevision: String = Pod0NMPBuild.testedRevision
    ) {
        storePath = storeURL.standardizedFileURL.path
        self.indexerRelays = Self.normalized(indexerRelays)
        appRelays = Self.normalized(operatorRelay.map { [$0] } ?? [])
        self.fallbackRelays = Self.normalized(fallbackRelays)
        self.allowedLocalRelayHosts = Array(Set(allowedLocalRelayHosts.map {
            $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        }.filter { !$0.isEmpty })).sorted()
        self.limits = limits
        self.nmpRevision = nmpRevision
    }

    func stagingOperatorRelay(_ relay: String?) -> Pod0NMPConfiguration {
        Pod0NMPConfiguration(
            storeURL: URL(fileURLWithPath: storePath),
            indexerRelays: indexerRelays,
            operatorRelay: relay,
            fallbackRelays: fallbackRelays,
            allowedLocalRelayHosts: allowedLocalRelayHosts,
            limits: limits,
            nmpRevision: nmpRevision
        )
    }

    private static func normalized(_ relays: [String]) -> [String] {
        Array(Set(relays.compactMap { raw -> String? in
            let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard let url = URL(string: value), ["wss", "ws"].contains(url.scheme?.lowercased()) else {
                return nil
            }
            return value
        })).sorted()
    }
}

