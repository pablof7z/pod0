import CarPlay
import Foundation
import UIKit
import os.log

// MARK: - CarPlaySceneDelegate
//
// Entry point for the CarPlay scene (`CPTemplateApplicationSceneSessionRoleApplication`).
// iOS instantiates this class when the head unit connects — the wiring is
// declared in `Info.plist` under `UIApplicationSceneManifest` and resolved
// against `$(PRODUCT_MODULE_NAME).CarPlaySceneDelegate`.
//
// Responsibilities:
//
//   1. Stand up a `CPTabBarTemplate` with a Library tab (subscribed
//      podcasts → per-show episode list) and surface playback through
//      the system-owned `CPNowPlayingTemplate`.
//   2. Bridge user taps into kernel actions via `KernelModel.shared`.
//      The scene delegate runs as a `UIResponder` outside the SwiftUI
//      environment chain, so we reach the live model through the
//      process-wide weak handle.
//   3. Refresh the template tree when the kernel snapshot's library
//      changes (new subscription, new episode), so a driver who stays
//      connected sees updates without reconnecting the head unit.
//
// Per AGENTS.md (soft 300 / hard 500 line limits), template construction
// lives in `CarPlayLibraryTemplates` and `CarPlayNowPlayingConfig`. This
// file is orchestration only.
//
// D6: never throws. A missing `KernelModel.shared` (CarPlay connected
// before the iPhone scene started the kernel) degrades to a "waiting"
// placeholder; we re-install the root template when the model and its
// library land.
// D7: this delegate reports + executes. Taps dispatch kernel actions;
// the kernel decides what to play / pause / seek. No client-side state.

@MainActor
final class CarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate {

    nonisolated private static let logger = Logger(
        subsystem: "io.f7z.podcast",
        category: "CarPlaySceneDelegate")

    private var interfaceController: CPInterfaceController?

    /// Library snapshot rev the template tree was last built against.
    /// `0` means no template installed; advancing it triggers a rebuild.
    private var lastBuiltLibraryRev: Int = 0

    /// Long-lived polling task that watches `KernelModel.shared` for
    /// library or now-playing changes. CarPlay scenes don't have
    /// `@Observable` propagation; a 2-second tick is well under any
    /// human-noticeable lag while staying cheap.
    private var refreshTask: Task<Void, Never>?

    // MARK: - CPTemplateApplicationSceneDelegate

    func templateApplicationScene(
        _ templateApplicationScene: CPTemplateApplicationScene,
        didConnect interfaceController: CPInterfaceController
    ) {
        Self.logger.info("CarPlay scene connected")
        self.interfaceController = interfaceController
        installRootTemplate()
        startRefreshLoop()
    }

    func templateApplicationScene(
        _ templateApplicationScene: CPTemplateApplicationScene,
        didDisconnectInterfaceController interfaceController: CPInterfaceController
    ) {
        Self.logger.info("CarPlay scene disconnected")
        self.interfaceController = nil
        refreshTask?.cancel()
        refreshTask = nil
        lastBuiltLibraryRev = 0
    }

    // MARK: - Template assembly

    /// Build the root tab bar from the live kernel snapshot. If the model
    /// is not yet available (cold-start race: CarPlay connected before the
    /// phone scene's `model.start()` landed a snapshot), show a placeholder
    /// and let `refreshLoop` reinstall on the next tick.
    private func installRootTemplate() {
        guard let interfaceController else { return }
        guard let model = KernelModel.shared, let snapshot = model.podcastSnapshot else {
            interfaceController.setRootTemplate(
                makeWaitingTemplate(), animated: false, completion: { _, _ in })
            return
        }
        let tabBar = makeTabBar(library: snapshot.library)
        interfaceController.setRootTemplate(tabBar, animated: false, completion: { _, _ in })
        CarPlayNowPlayingConfig.configure(interfaceController: interfaceController)
        lastBuiltLibraryRev = snapshot.rev
    }

    private func makeTabBar(library: [PodcastSummary]) -> CPTabBarTemplate {
        let libraryTab = CarPlayLibraryTemplates.makeLibraryTemplate(
            library: library,
            onSelectShow: { [weak self] podcast in
                self?.pushEpisodeList(for: podcast)
            })
        return CPTabBarTemplate(templates: [libraryTab])
    }

    private func pushEpisodeList(for podcast: PodcastSummary) {
        guard let interfaceController else { return }
        let template = CarPlayLibraryTemplates.makeEpisodesTemplate(
            podcast: podcast,
            onSelectEpisode: { [weak self] episode in
                self?.startPlayback(episode: episode)
            })
        interfaceController.pushTemplate(template, animated: true, completion: { _, _ in })
    }

    // MARK: - Playback dispatch

    /// Dispatch `podcast.player.play` for the selected episode, then push
    /// the system-owned `CPNowPlayingTemplate` so the driver lands on
    /// familiar transport controls.
    private func startPlayback(episode: EpisodeSummary) {
        guard let model = KernelModel.shared else {
            Self.logger.error("startPlayback: KernelModel.shared is nil")
            return
        }
        model.dispatch(namespace: "podcast.player", body: [
            "op": "play",
            "episode_id": episode.id
        ])
        if let interfaceController {
            interfaceController.pushTemplate(
                CPNowPlayingTemplate.shared, animated: true, completion: { _, _ in })
        }
    }

    // MARK: - Live refresh loop

    /// Re-install the root template when the library snapshot's `rev`
    /// changes. Also rebuilds when the model becomes available after a
    /// cold-start race (the initial installRoot painted a placeholder).
    private func startRefreshLoop() {
        refreshTask?.cancel()
        refreshTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                self?.refreshIfNeeded()
            }
        }
    }

    private func refreshIfNeeded() {
        guard let model = KernelModel.shared,
              let snapshot = model.podcastSnapshot
        else { return }
        guard snapshot.rev != lastBuiltLibraryRev else { return }
        installRootTemplate()
    }

    // MARK: - Cold-connect placeholder

    private func makeWaitingTemplate() -> CPTemplate {
        let item = CPListItem(text: "Loading your podcasts…", detailText: nil)
        let section = CPListSection(items: [item])
        let template = CPListTemplate(title: "Pod0", sections: [section])
        template.tabImage = UIImage(systemName: "books.vertical")
        template.tabTitle = "Library"
        return template
    }
}
