import Foundation
import Observation
import os.log

// MARK: - BriefingsViewModel

/// Owns the briefing list + active compose state for `BriefingsView` and the
/// player surface. Self-contained — no dependency on `AppStateStore` so this
/// lane stays orthogonal to the rest of the app.
///
/// The view model wires up real `BriefingComposer` + `BriefingStorage`, so
/// the data flow under the UI is fully exercised even when the LLM and TTS
/// fall back to fakes.
@MainActor
@Observable
final class BriefingsViewModel {

    // MARK: Public state

    /// Briefings already on disk, sorted newest-first.
    private(set) var briefings: [BriefingScript] = []

    /// Streaming progress emitted while the composer runs. `nil` when idle.
    private(set) var composeProgress: BriefingComposeProgress?

    /// Most recent compose error, if any. Cleared when a new compose starts.
    private(set) var composeError: String?

    /// `true` while a compose run is in flight.
    var isComposing: Bool { composeProgress != nil && composeProgress != .finished }

    /// `true` when briefings are stored in `temporaryDirectory` because
    /// `Application Support` was unavailable. Data will be lost on relaunch.
    private(set) var isUsingEphemeralStorage: Bool = false

    // MARK: Logger

    private static let logger = Logger.app("BriefingsViewModel")

    // MARK: Dependencies

    private let storage: BriefingStorage
    private let composer: BriefingComposing

    // MARK: Init

    init(
        storage: BriefingStorage? = nil,
        composer: BriefingComposing? = nil
    ) {
        let resolvedStorage: BriefingStorage
        var ephemeral = false
        if let s = storage {
            resolvedStorage = s
        } else if let s = try? BriefingStorage() {
            resolvedStorage = s
        } else {
            // Fall back to a temporary directory if Application Support is
            // unavailable (rare; mostly happens in restricted preview hosts).
            Self.logger.error("Application Support unavailable — BriefingsViewModel falling back to temporaryDirectory. Briefings will NOT survive app relaunch.")
            let tmp = FileManager.default.temporaryDirectory
                .appendingPathComponent("briefings-\(UUID().uuidString)", isDirectory: true)
            // swiftlint:disable:next force_try
            resolvedStorage = try! BriefingStorage(rootDirectory: tmp)
            ephemeral = true
        }
        self.storage = resolvedStorage
        self.composer = composer ?? BriefingComposer(storage: resolvedStorage)
        self.isUsingEphemeralStorage = ephemeral
    }

    // MARK: Library

    func reload() async {
        briefings = (try? storage.listScripts()) ?? []
    }

    func delete(_ script: BriefingScript) {
        try? storage.delete(id: script.id)
        briefings.removeAll { $0.id == script.id }
    }

    // MARK: Compose

    /// Run a freeform compose. Updates `composeProgress` as the composer
    /// emits stages, then refreshes `briefings`.
    func compose(request: BriefingRequest) async {
        composeProgress = .selectedEpisodes(count: 0)
        composeError = nil
        do {
            _ = try await composer.compose(request: request) { [weak self] progress in
                Task { @MainActor in
                    self?.composeProgress = progress
                }
            }
        } catch {
            composeError = error.localizedDescription
        }
        composeProgress = nil
        await reload()
    }

    /// Convenience for the preset row: compose a default-shape briefing for a
    /// given style (8 minutes, my-subscriptions scope).
    func composeQuick(style: BriefingStyle) async {
        let request = BriefingRequest(scope: .mySubscriptions, length: .medium, style: style)
        await compose(request: request)
    }
}
