import Combine
import Foundation
import os.log

enum UsageLedgerPersistenceStatus: Equatable {
    case ready
    case unavailable
}

@MainActor
final class CostLedger: ObservableObject {
    static let shared = CostLedger()
    static let maximumRecordCount = 500
    static let retentionDays = 90

    @Published private(set) var records: [UsageRecord]
    @Published private(set) var persistenceStatus: UsageLedgerPersistenceStatus

    private let directoryURL: URL
    private let fileURL: URL
    private let now: () -> Date
    private static let logger = Logger.app("CostLedger")

    private convenience init() {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        self.init(fileURL: base
            .appendingPathComponent("UsageLedger", isDirectory: true)
            .appendingPathComponent("ledger.json"))
    }

    init(fileURL: URL, now: @escaping () -> Date = Date.init) {
        self.fileURL = fileURL
        directoryURL = fileURL.deletingLastPathComponent()
        self.now = now
        do {
            try FileManager.default.createDirectory(
                at: directoryURL,
                withIntermediateDirectories: true
            )
            let loaded = try Self.load(from: fileURL)
            records = Self.retained(loaded, now: now())
            persistenceStatus = .ready
            if records != loaded { save() }
        } catch {
            records = []
            persistenceStatus = .unavailable
            Self.logger.error("Usage ledger load failed; source preserved until explicit reset")
        }
    }

    func log(
        feature: String,
        model: String,
        usage: OpenRouterUsagePayload?,
        latencyMs: Int
    ) {
        append(UsageRecord(
            id: UUID(),
            at: now(),
            feature: feature,
            model: model,
            promptTokens: usage?.prompt_tokens ?? 0,
            completionTokens: usage?.completion_tokens ?? 0,
            cachedTokens: usage?.prompt_tokens_details?.cached_tokens ?? 0,
            reasoningTokens: usage?.completion_tokens_details?.reasoning_tokens ?? 0,
            costUSD: usage?.cost ?? 0,
            latencyMs: latencyMs
        ))
    }

    func logOllama(
        feature: String,
        model: String,
        promptTokens: Int,
        completionTokens: Int,
        latencyMs: Int
    ) {
        append(UsageRecord(
            id: UUID(),
            at: now(),
            feature: feature,
            model: model,
            promptTokens: promptTokens,
            completionTokens: completionTokens,
            cachedTokens: 0,
            reasoningTokens: 0,
            costUSD: 0,
            latencyMs: latencyMs
        ))
    }

    /// STT-shaped record: audio duration in seconds + optional cost. Use this
    /// from `AssemblyAITranscriptClient`, `ElevenLabsScribeClient`, and
    /// `OpenRouterWhisperClient` to record transcription activity. Cost may be
    /// zero when the provider's response doesn't surface it — the entry still
    /// appears in the Usage view so the user has a unified activity log.
    func logSTT(
        feature: String,
        model: String,
        costUSD: Double,
        audioDurationSeconds: Double?,
        latencyMs: Int,
        promptTokens: Int = 0,
        completionTokens: Int = 0
    ) {
        append(UsageRecord(
            id: UUID(),
            at: now(),
            feature: feature,
            model: model,
            promptTokens: promptTokens,
            completionTokens: completionTokens,
            cachedTokens: 0,
            reasoningTokens: 0,
            costUSD: costUSD,
            latencyMs: latencyMs,
            audioDurationSeconds: audioDurationSeconds
        ))
    }

    func clear() {
        records = []
        persistenceStatus = .ready
        save()
    }

    private func append(_ record: UsageRecord) {
        guard persistenceStatus == .ready else { return }
        records.insert(record, at: 0)
        records = Self.retained(records, now: now())
        save()
    }

    private func save() {
        do {
            let data = try Self.encoder.encode(records)
            try data.write(
                to: fileURL,
                options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
            )
            persistenceStatus = .ready
        } catch {
            persistenceStatus = .unavailable
            Self.logger.error("Usage ledger save failed")
        }
    }

    /// Configured once. `save()` runs on every cost-log (every LLM
    /// call the user makes), so per-call encoder construction +
    /// `.iso8601` / `.sortedKeys` configuration was real (if modest)
    /// waste. Matches the same fix applied to `AgentRunLogger.save()`.
    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
    }()

    private static func load(from url: URL) throws -> [UsageRecord] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode([UsageRecord].self, from: data)
    }

    private static func retained(_ records: [UsageRecord], now: Date) -> [UsageRecord] {
        let cutoff = now.addingTimeInterval(-Double(retentionDays) * 86_400)
        return records
            .filter { $0.at >= cutoff }
            .sorted { $0.at > $1.at }
            .prefix(maximumRecordCount)
            .map(\.withoutContent)
    }
}
