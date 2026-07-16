import os.log
import SwiftUI

#if canImport(NMP)
import NMP
#endif

/// The top-level entry point for the app. Sets up global environment objects.
@main
struct PodcastrApp: App {
    nonisolated private static let nmpLogger = Logger.app("Pod0NMP")

    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var store = AppStateStore()
    @State private var userIdentity = UserIdentityStore.shared
    @State private var scheduledTaskRunner: AgentScheduledTaskRunner?
    #if canImport(NMP)
    @State private var nmpComposition: Pod0NMPComposition?
    #endif
    /// Single global owner-consultation coordinator. Lives here (not on
    /// `AgentChatSession`) so an inbound peer-agent reply flowing through
    /// `AgentRelayBridge` can pop the same sheet even when the user is on
    /// Home / Library / Wiki — i.e. while no chat session exists. Mounted
    /// on `RootView` via `agentAskPresenter(coordinator:)`.
    @State private var askCoordinator = AgentAskCoordinator()

    // MARK: - What's-new sheet wiring
    //
    // Evaluated once on cold launch (`.task` below). Stays here in
    // `AppMain.swift` rather than `RootView.swift` so the "what changed
    // since you last opened the app" check fires before any tab-level
    // view has a chance to short-circuit it.
    //
    // Single optional `@State` + `.sheet(item:)` rather than the more
    // common pair of `entries: [...]` and `isPresented: Bool`. The
    // `OnboardingView` fullScreenCover sits on top of RootView during
    // first launch, and SwiftUI re-evaluates the queued sheet's content
    // closure once the cover dismisses. With the two-state pattern the
    // closure was reading a stale `entries = []` from across that
    // render boundary, so the sheet rendered empty. `.sheet(item:)`
    // passes the entries through the trigger itself, eliminating the
    // race.
    @State private var whatsNewPresentation: WhatsNewPresentation?

    var body: some Scene {
        WindowGroup {
            RootView(scheduledTaskRunner: scheduledTaskRunner)
                .environment(store)
                .environment(userIdentity)
                .environment(askCoordinator)
                .task { CarPlayController.shared.attach(store: store) }
                #if canImport(NMP)
                .task { await startNMPIfNeeded() }
                #endif
                .task {
                    scheduledTaskRunner = AgentScheduledTaskRunner(store: store)
                }
                .task {
                    // Seed a fresh install silently so the first launch
                    // doesn't dump the entire changelog as "new."
                    WhatsNewService.seedIfNeeded()
                    let unseen = WhatsNewService.unseenEntries(
                        lastSeenAt: WhatsNewService.lastSeenAt
                    )
                    if !unseen.isEmpty {
                        whatsNewPresentation = WhatsNewPresentation(entries: unseen)
                    }
                }
                .task {
                    // One-shot backfill: ensure every episode the user
                    // already has in the library has its title +
                    // description embedded so `find_similar_episodes` /
                    // `search_episodes` can surface untranscribed
                    // episodes. Throttled + reentrancy-safe; subsequent
                    // launches no-op once everything is flagged.
                    await EpisodeMetadataIndexer.shared.runBackfill(appStore: store)
                }
                .sheet(item: $whatsNewPresentation) { presentation in
                    WhatsNewSheet(entries: presentation.entries)
                }
        }
    }

    #if canImport(NMP)
    /// Starts exactly one clean NMP store owner for this process. Product
    /// slices receive this retained composition; no legacy state is imported.
    private func startNMPIfNeeded() async {
        guard nmpComposition == nil else { return }
        do {
            let layout = try Pod0NMPStoreLayout.applicationSupport()
            let settings = store.state.settings
            let configuration = Pod0NMPConfiguration(
                storeURL: layout.storeURL,
                indexerRelays: [],
                operatorRelay: settings.nostrEnabled ? settings.nostrRelayURL : nil,
                fallbackRelays: []
            )
            let composition = try Pod0NMPComposition(
                configuration: configuration,
                layout: layout,
                localAccountStore: NMPKeychainAccountStore(
                    service: Pod0HumanIdentityLifecycle.localKeychainService(
                        bundleIdentifier: Bundle.main.bundleIdentifier ?? "Podcastr"
                    ),
                    account: Pod0HumanIdentityLifecycle.localSecretReference
                )
            )
            nmpComposition = composition
            await userIdentity.start(composition: composition)
        } catch {
            Self.nmpLogger.error("NMP startup failed closed: \(String(describing: error), privacy: .public)")
        }
    }
    #endif
}

/// Drives the What's New `.sheet(item:)`. Bundling the entries with the
/// trigger guarantees the sheet content closure receives them atomically
/// — see the wiring note above.
private struct WhatsNewPresentation: Identifiable {
    let id = UUID()
    let entries: [WhatsNewEntry]
}
