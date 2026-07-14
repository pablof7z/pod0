import CarPlay
import Foundation
import UIKit
import os.log

// MARK: - CarPlaySceneDelegate
//
// Entry point for the CarPlay scene. iOS instantiates this class (the name
// is wired in `Info.plist` under `CPTemplateApplicationSceneSessionRoleApplication`)
// when the head unit connects. Our job is to:
//
//   1. Stand up the root tab bar template (Listen Now / Shows / Downloads).
//   2. Configure the system-owned `CPNowPlayingTemplate` with our custom
//      buttons (speed cycle + chapters).
//   3. Re-push the root template if the live context wasn't ready at connect
//      time — happens when CarPlay connects faster than `RootView.onAppear`.
//
// Per the AGENTS.md 300-line soft limit, the per-tab template construction
// lives in dedicated builder enums (CarPlayListenNow, CarPlayShows,
// CarPlayDownloads, CarPlayNowPlaying). This file stays focused on
// orchestration: scene lifecycle + selection handlers that talk to
// `PlaybackState` and push templates.

@MainActor
final class CarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate {

    nonisolated private static let logger = Logger.app("CarPlaySceneDelegate")

    private var interfaceController: CPInterfaceController?
    private var contextReadyObserver: NSObjectProtocol?
    /// Tracks the last episode the now-playing buttons were configured for
    /// so a chapter-set hydration triggers a single refresh, not one per tick.
    private var lastNowPlayingEpisodeID: UUID?
    /// Long-lived polling task that watches for episode changes so the
    /// chapter button can appear / hide as the loaded episode swaps.
    private var pollTask: Task<Void, Never>?

    // MARK: - CPTemplateApplicationSceneDelegate

    func templateApplicationScene(
        _ templateApplicationScene: CPTemplateApplicationScene,
        didConnect interfaceController: CPInterfaceController
    ) {
        Self.logger.info("CarPlay scene connected")
        self.interfaceController = interfaceController
        installRootTemplate()
        startObservingPlaybackForButtonRefresh()
    }

    func templateApplicationScene(
        _ templateApplicationScene: CPTemplateApplicationScene,
        didDisconnectInterfaceController interfaceController: CPInterfaceController
    ) {
        Self.logger.info("CarPlay scene disconnected")
        self.interfaceController = nil
        pollTask?.cancel()
        pollTask = nil
        if let token = contextReadyObserver {
            NotificationCenter.default.removeObserver(token)
            contextReadyObserver = nil
        }
    }

    // MARK: - Template assembly

    private func installRootTemplate() {
        guard let interfaceController else { return }
        guard let store = CarPlayController.shared.store,
              let playback = CarPlayController.shared.playback
        else {
            // Race: CarPlay connected before the iPhone scene finished
            // attaching store/playback. Show a placeholder and re-install
            // when the context lands.
            interfaceController.setRootTemplate(makeWaitingTemplate(), animated: false) { _, _ in }
            observeContextReady()
            return
        }

        let tabBar = makeTabBar(store: store, playback: playback)
        interfaceController.setRootTemplate(tabBar, animated: false) { _, _ in }
        CarPlayNowPlaying.configure(playback: playback, interfaceController: interfaceController)
    }

    private func makeTabBar(store: AppStateStore, playback: PlaybackState) -> CPTabBarTemplate {
        let listenNow = CarPlayListenNow.makeTemplate(store: store) { [weak self] episode in
            self?.startPlayback(episode: episode, playback: playback, store: store)
        }
        let shows = CarPlayShows.makeRootTemplate(store: store) { [weak self] podcast in
            self?.pushShowEpisodes(podcast: podcast, store: store, playback: playback)
        }
        let downloads = CarPlayDownloads.makeTemplate(store: store) { [weak self] episode in
            self?.startPlayback(episode: episode, playback: playback, store: store)
        }
        return CPTabBarTemplate(templates: [listenNow, shows, downloads])
    }

    /// Push the per-show episode list for `podcast`.
    private func pushShowEpisodes(
        podcast: Podcast,
        store: AppStateStore,
        playback: PlaybackState
    ) {
        guard let interfaceController else { return }
        let template = CarPlayShows.makeEpisodesTemplate(for: podcast, store: store) { [weak self] episode in
            self?.startPlayback(episode: episode, playback: playback, store: store)
        }
        interfaceController.pushTemplate(template, animated: true) { _, _ in }
    }

    /// Load + play an episode, then surface the Now Playing template so the
    /// driver lands on familiar transport controls. Order matters — CarPlay
    /// only animates the spinner on the originating list row until we push.
    private func startPlayback(
        episode: Episode,
        playback: PlaybackState,
        store: AppStateStore
    ) {
        playback.setEpisode(episode)
        playback.play()
        // Refresh the chapter button against the freshly-loaded episode
        // before we land on Now Playing — the user shouldn't see a stale
        // chapter list for the previous episode.
        if let interfaceController {
            CarPlayNowPlaying.refresh(playback: playback, interfaceController: interfaceController)
            interfaceController.pushTemplate(CPNowPlayingTemplate.shared, animated: true) { _, _ in }
        }
        lastNowPlayingEpisodeID = episode.id
    }

    // MARK: - Live refresh

    /// Chapters can hydrate post-load (the player fetches the JSON in the
    /// background). Poll on a slow cadence so the chapter button appears /
    /// the chapter list freshens without us subscribing to `@Observable`
    /// changes from a non-SwiftUI scene. 3-second tick is well below human
    /// notice on a road trip but doesn't waste cycles.
    private func startObservingPlaybackForButtonRefresh() {
        pollTask?.cancel()
        pollTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3))
                self?.refreshNowPlayingIfEpisodeChanged()
            }
        }
    }

    private func refreshNowPlayingIfEpisodeChanged() {
        guard let interfaceController,
              let playback = CarPlayController.shared.playback else { return }
        let currentID = playback.episode?.id
        guard currentID != lastNowPlayingEpisodeID else { return }
        lastNowPlayingEpisodeID = currentID
        CarPlayNowPlaying.refresh(playback: playback, interfaceController: interfaceController)
    }

    // MARK: - Cold-connect waiting state

    private func makeWaitingTemplate() -> CPTemplate {
        let item = CPListItem(text: "Loading your podcasts…", detailText: nil)
        let section = CPListSection(items: [item])
        let template = CPListTemplate(title: "Podcastr", sections: [section])
        return template
    }

    private func observeContextReady() {
        guard contextReadyObserver == nil else { return }
        contextReadyObserver = NotificationCenter.default.addObserver(
            forName: CarPlayController.contextReady,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.installRootTemplate()
            }
        }
    }
}
