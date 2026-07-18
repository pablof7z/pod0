import Foundation
import os.log

actor ProductSignalStore: ProductSignalSink {
    static let shared = ProductSignalStore()
    static let maxSignalCount = 10_000

    private struct Archive: Codable {
        var schemaVersion: Int
        var anonymousInstallID: UUID
        var isEnabled: Bool
        var activeSessionID: UUID?
        var signals: [ProductSignal]
    }

    private static let logger = Logger.app("ProductSignalStore")
    private let fileURL: URL
    private var archive: Archive
    private var signalIDs: Set<UUID>
    private var processSessionID: UUID?

    init(fileURL: URL? = nil) {
        self.fileURL = fileURL ?? Self.defaultFileURL()
        archive = Self.load(from: self.fileURL) ?? Archive(
            schemaVersion: ProductSignal.currentSchemaVersion,
            anonymousInstallID: UUID(),
            isEnabled: true,
            activeSessionID: nil,
            signals: []
        )
        signalIDs = Set(archive.signals.map(\.id))
    }

    func record(_ observation: ProductSignalObservation) async {
        guard archive.isEnabled, signalIDs.insert(observation.signalID).inserted else { return }
        archive.signals.append(ProductSignal(
            observation: observation,
            anonymousInstallID: archive.anonymousInstallID
        ))
        if archive.signals.count > Self.maxSignalCount {
            let excess = archive.signals.count - Self.maxSignalCount
            let removed = Array(archive.signals.prefix(excess))
            archive.signals.removeFirst(excess)
            signalIDs.subtract(removed.map(\.id))
        }
        persistFailOpen()
    }

    func snapshot() -> ProductSignalSnapshot {
        ProductSignalSnapshot(
            isEnabled: archive.isEnabled,
            anonymousInstallID: archive.anonymousInstallID,
            signals: archive.signals.sorted { $0.occurredAt > $1.occurredAt }
        )
    }

    func setEnabled(_ enabled: Bool) {
        guard archive.isEnabled != enabled else { return }
        archive.isEnabled = enabled
        if !enabled {
            archive.signals.removeAll()
            signalIDs.removeAll()
            archive.anonymousInstallID = UUID()
            archive.activeSessionID = nil
            processSessionID = nil
        }
        persistFailOpen()
    }

    func deleteAll() async {
        archive.signals.removeAll()
        signalIDs.removeAll()
        archive.anonymousInstallID = UUID()
        persistFailOpen()
    }

    func setSessionActive(_ isActive: Bool, now: Date = Date()) async {
        guard archive.isEnabled else { return }
        if isActive {
            guard processSessionID == nil else { return }
            if archive.activeSessionID != nil {
                await record(.init(
                    occurredAt: now,
                    name: .uncleanTermination,
                    outcome: .detected,
                    errorClass: .unexpected
                ))
            }
            let sessionID = UUID()
            processSessionID = sessionID
            archive.activeSessionID = sessionID
            await record(.init(occurredAt: now, name: .appLaunch, outcome: .started))
            persistFailOpen()
        } else if archive.activeSessionID == processSessionID {
            archive.activeSessionID = nil
            processSessionID = nil
            persistFailOpen()
        }
    }

    func exportData(now: Date = Date()) -> Data? {
        let payload = ProductSignalExport(
            schemaVersion: ProductSignal.currentSchemaVersion,
            privacy: "Content-free local product signals; manually exported by the user.",
            report: ProductSignalReport(signals: archive.signals, generatedAt: now),
            signals: archive.signals
        )
        return try? Self.encoder.encode(payload)
    }

    private func persistFailOpen() {
        do {
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try Self.encoder.encode(archive).write(to: fileURL, options: .atomic)
        } catch {
            Self.logger.error("Product signal persistence failed")
        }
    }

    private static func load(from url: URL) -> Archive? {
        guard let data = try? Data(contentsOf: url),
              let archive = try? decoder.decode(Archive.self, from: data),
              archive.schemaVersion == ProductSignal.currentSchemaVersion else { return nil }
        return archive
    }

    private static func defaultFileURL() -> URL {
        let support = (try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )) ?? FileManager.default.temporaryDirectory
        return support.appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("product-signals-v1.json")
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
