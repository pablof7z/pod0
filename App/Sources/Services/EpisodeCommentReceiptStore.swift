import Foundation

struct PendingEpisodeCommentReceipt: Codable, Equatable, Identifiable, Sendable {
    var id: UInt64 { receiptID }

    let receiptID: UInt64
    let target: CommentTarget
    let content: String
    let submittedAt: Date
}

protocol EpisodeCommentReceiptStore: Sendable {
    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt]
    func save(_ record: PendingEpisodeCommentReceipt)
    func remove(receiptID: UInt64)
}

/// App-owned durable index of NMP receipt ids. NMP owns the durable outbox;
/// this small index exists only so Pod0 can reattach UI observers after launch.
final class UserDefaultsEpisodeCommentReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let defaults: UserDefaults
    private let key: String
    private let lock = NSLock()

    init(defaults: UserDefaults = .standard, key: String = "episode-comment-receipts-v1") {
        self.defaults = defaults
        self.key = key
    }

    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt] {
        lock.withLock { load().filter { $0.target == target } }
    }

    func save(_ record: PendingEpisodeCommentReceipt) {
        lock.withLock {
            var records = load()
            records.removeAll { $0.receiptID == record.receiptID }
            records.append(record)
            persist(records)
        }
    }

    func remove(receiptID: UInt64) {
        lock.withLock {
            var records = load()
            records.removeAll { $0.receiptID == receiptID }
            persist(records)
        }
    }

    private func load() -> [PendingEpisodeCommentReceipt] {
        guard let data = defaults.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([PendingEpisodeCommentReceipt].self, from: data)) ?? []
    }

    private func persist(_ records: [PendingEpisodeCommentReceipt]) {
        guard !records.isEmpty else {
            defaults.removeObject(forKey: key)
            return
        }
        if let data = try? JSONEncoder().encode(records) {
            defaults.set(data, forKey: key)
        }
    }
}
