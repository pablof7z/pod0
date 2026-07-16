import Foundation

struct PendingEpisodeCommentReceipt: Codable, Equatable, Identifiable, Sendable {
    var id: UInt64 { receiptID }

    let receiptID: UInt64
    let target: CommentTarget
    /// Stable correlation learned from NMP after signing. Draft/event content
    /// is deliberately absent: NMP's canonical pending row is the only event
    /// projection rendered by the comments view.
    var eventID: String?
    let submittedAt: Date
}

protocol EpisodeCommentReceiptStore: Sendable {
    func records(for target: CommentTarget) throws -> [PendingEpisodeCommentReceipt]
    func save(_ record: PendingEpisodeCommentReceipt) throws
    func remove(receiptID: UInt64) throws
    func removeAll()
}

enum EpisodeCommentReceiptStoreError: LocalizedError {
    case unreadable

    var errorDescription: String? {
        "Pending comment delivery receipts could not be read. They were left untouched."
    }
}

/// App-owned durable index of NMP receipt ids. NMP owns the durable outbox;
/// this small index exists only so Pod0 can reattach UI observers after launch.
final class UserDefaultsEpisodeCommentReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let defaults: UserDefaults
    private let key: String
    private let lock = NSLock()

    init(defaults: UserDefaults = .standard, key: String = "episode-comment-receipts-v2") {
        self.defaults = defaults
        self.key = key
    }

    func records(for target: CommentTarget) throws -> [PendingEpisodeCommentReceipt] {
        try lock.withLock { try load().filter { $0.target == target } }
    }

    func save(_ record: PendingEpisodeCommentReceipt) throws {
        try lock.withLock {
            var records = try load()
            records.removeAll { $0.receiptID == record.receiptID }
            records.append(record)
            try persist(records)
        }
    }

    func remove(receiptID: UInt64) throws {
        try lock.withLock {
            var records = try load()
            records.removeAll { $0.receiptID == receiptID }
            try persist(records)
        }
    }

    func removeAll() {
        lock.withLock { defaults.removeObject(forKey: key) }
    }

    private func load() throws -> [PendingEpisodeCommentReceipt] {
        guard let data = defaults.data(forKey: key) else { return [] }
        do {
            return try JSONDecoder().decode([PendingEpisodeCommentReceipt].self, from: data)
        } catch {
            throw EpisodeCommentReceiptStoreError.unreadable
        }
    }

    private func persist(_ records: [PendingEpisodeCommentReceipt]) throws {
        guard !records.isEmpty else {
            defaults.removeObject(forKey: key)
            return
        }
        defaults.set(try JSONEncoder().encode(records), forKey: key)
    }
}
