import SwiftUI

// ─────────────────────────────────────────────────────────────────────────
// NAVIGATION CONTRACT — M0.B skeleton.
//
// Single placeholder tab (Pod0) until Feature views migrate in later
// milestones. The toast overlay and kernel-dead banner are wired now so
// every future feature screen inherits them without modification.
//
// Navigation: one NavigationStack per tab. Typed `PodcastRoute` destinations
// are resolved centrally here so Profile/Episode/Show work identically from
// any tab without cross-file coupling.
// ─────────────────────────────────────────────────────────────────────────

/// Typed navigation routes for the Podcast shell.
enum PodcastRoute: Hashable {
    // Populated with episode, show, and feed routes in later milestones.
}

/// Per-tab navigation path holder injected into the environment.
@MainActor
final class PodcastRouter: ObservableObject {
    @Published var path = NavigationPath()
    func push(_ r: PodcastRoute) { path.append(r) }
    func popToRoot() { path = NavigationPath() }
}

enum PodcastTab: Hashable { case library, downloads, identity }
enum PodcastTab: Hashable { case library, briefings, identity }

struct RootShell: View {
    @Environment(KernelModel.self) private var model
    @Environment(SpotlightDeepLinkRouter.self) private var deepLinkRouter

    @State private var tab: PodcastTab = .library

    var body: some View {
        mainTabs
            .podcastScreenBackground()
            .overlay(alignment: .top) { toast }
            // D7 actor-death banner — rendered on top of everything,
            // non-dismissible. See kernelDeadBanner for full rationale.
            .overlay(alignment: .top) { kernelDeadBanner }
            // Spotlight tap → switch to library tab. The actual
            // `NavigationStack` push happens inside `LibraryView`,
            // which observes the same router and consumes the
            // pending deep link after pushing. We only own the tab
            // dimension here; the navigation stack is per-tab.
            .onChange(of: deepLinkRouter.pendingDeepLink) { _, newValue in
                guard newValue != nil else { return }
                tab = .library
            }
    }

    private var mainTabs: some View {
        TabView(selection: $tab) {
            LibraryView()
                .tabItem { Label("Library", systemImage: "books.vertical") }
                .tag(PodcastTab.library)

            DownloadsView()
                .tabItem { Label("Downloads", systemImage: "arrow.down.circle") }
                .tag(PodcastTab.downloads)
            BriefingsView()
                .tabItem { Label("Briefings", systemImage: "newspaper") }
                .tag(PodcastTab.briefings)

            IdentityRootView()
                .tabItem { Label("Identity", systemImage: "person.circle") }
                .tag(PodcastTab.identity)
        }
        .toolbarBackground(.visible, for: .tabBar)
        .toolbarBackground(.regularMaterial, for: .tabBar)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            MiniPlayerView()
        }
    }

    @ViewBuilder
    private func tabStack<Root: View>(@ViewBuilder _ root: () -> Root) -> some View {
        TabStack(root: root())
    }

    @ViewBuilder
    private var toast: some View {
        if let msg = model.lastErrorToast {
            Text(msg)
                .font(PodcastFont.callout)
                .foregroundStyle(.primary)
                .padding(.horizontal, PodcastSpace.l).padding(.vertical, PodcastSpace.m)
                .background(.regularMaterial, in: Capsule())
                .padding(.top, 8)
                .onTapGesture { model.clearErrorToast() }
                .task {
                    try? await Task.sleep(for: .seconds(4))
                    model.clearErrorToast()
                }
        }
    }

    // D7 actor-death banner. The Rust actor thread that owns the kernel loop
    // has died (panic or liveness-probe failure on resume, ADR-0028). Every
    // subsequent FFI call is a silent no-op; the only safe recovery is a
    // process restart. The banner is full-width danger, non-dismissible.
    @ViewBuilder
    private var kernelDeadBanner: some View {
        if model.kernelIsDead {
            VStack(alignment: .leading, spacing: PodcastSpace.s) {
                Text("Background service stopped")
                    .font(PodcastFont.headline)
                    .foregroundStyle(PodcastColor.emphasisForeground)
                Text("Please relaunch the app to recover.")
                    .font(PodcastFont.callout)
                    .foregroundStyle(PodcastColor.emphasisForeground.opacity(0.92))
                Button {
                    exit(0)
                } label: {
                    Text("Relaunch")
                        .font(PodcastFont.callout.weight(.semibold))
                        .padding(.horizontal, PodcastSpace.l)
                        .padding(.vertical, PodcastSpace.s)
                        .background(PodcastColor.emphasisForeground, in: Capsule())
                        .foregroundStyle(PodcastColor.danger)
                }
                .accessibilityIdentifier("kernel-dead-relaunch-button")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, PodcastSpace.l)
            .padding(.vertical, PodcastSpace.m)
            .background(PodcastColor.errorBannerBackground)
            .accessibilityIdentifier("kernel-dead-banner")
        }
    }
}

private struct TabStack<Root: View>: View {
    let root: Root
    @StateObject private var router = PodcastRouter()
    var body: some View {
        NavigationStack(path: $router.path) {
            root
                .navigationDestination(for: PodcastRoute.self) { _ in
                    // Destinations wired in later milestones.
                    EmptyView()
                }
        }
        .environmentObject(router)
    }
}
