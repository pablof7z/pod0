import Foundation
import os.log

// MARK: - CarPlayController
//
// Singleton bridge that hands the live `AppStateStore` and `PlaybackState`
// instances owned by the SwiftUI scene to the CarPlay scene delegate.
//
// **Why a singleton.** CarPlay runs as a separate `UIScene` (the
// `CPTemplateApplicationScene` role) and can connect before the SwiftUI
// scene's `.onAppear` lands the @State instances. CarPlay can also connect
// when the phone is locked and the SwiftUI scene never runs at all. The
// store/playback instances we want to drive CarPlay from must therefore live
// somewhere CarPlay can reach without a SwiftUI environment chain.
//
// `AppMain.task` calls `attach(store:)` at startup; `RootView.onAppear`
// calls `attach(playback:)` once `PlaybackState` is constructed. Each
// attach posts `Notification.Name.carPlayContextReady` so an already-
// connected CarPlay scene rebuilds its templates when the data arrives.

@MainActor
final class CarPlayController {

    static let shared = CarPlayController()

    nonisolated private static let logger = Logger.app("CarPlayController")

    /// Live state-store handle. `nil` until `AppMain` finishes hydrating.
    private(set) var store: AppStateStore?
    /// Live playback handle. `nil` until `RootView` has constructed it.
    private(set) var playback: PlaybackState?

    /// Posted (once per attach) so a CarPlay scene that connected before the
    /// app's main scene finished initializing can rebuild its template tree
    /// against fresh data. Listeners must filter on `nil` payloads.
    static let contextReady = Notification.Name("CarPlayController.contextReady")

    private init() {}

    func attach(store: AppStateStore) {
        // Idempotent: a hot relaunch may call us twice as SwiftUI replays
        // `.task` on the same store instance. Same-reference is a no-op.
        if self.store === store { return }
        self.store = store
        Self.logger.info("CarPlay store attached")
        notifyReadyIfPossible()
    }

    func attach(playback: PlaybackState) {
        if self.playback === playback { return }
        self.playback = playback
        Self.logger.info("CarPlay playback attached")
        notifyReadyIfPossible()
    }

    /// True once both halves of the context have landed.
    var isReady: Bool { store != nil && playback != nil }

    private func notifyReadyIfPossible() {
        guard isReady else { return }
        NotificationCenter.default.post(name: Self.contextReady, object: nil)
    }
}
