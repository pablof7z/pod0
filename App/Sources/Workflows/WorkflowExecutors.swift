import Foundation

@MainActor
final class MetadataIndexJobExecutor: JobExecutor {
    init(store: AppStateStore) { _ = store }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        _ = context
        return .obsolete
    }
}
