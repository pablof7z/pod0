import SwiftUI

enum PodcastTab: Hashable { case home, library, downloads, social, inbox, agent, identity }

struct RootShell: View {
    @Environment(KernelModel.self) private var model
    @Environment(SpotlightDeepLinkRouter.self) private var deepLinkRouter

    @State private var tab: PodcastTab = .home

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
            HomeView()
                .tabItem { Label("Home", systemImage: "house") }
                .tag(PodcastTab.home)

            LibraryView()
                .tabItem { Label("Library", systemImage: "books.vertical") }
                .tag(PodcastTab.library)

            DownloadsView()
                .tabItem { Label("Downloads", systemImage: "arrow.down.circle") }
                .tag(PodcastTab.downloads)
            SocialView()
                .tabItem { Label("Social", systemImage: "person.2") }
                .tag(PodcastTab.social)
            InboxView()
                .tabItem { Label("Inbox", systemImage: "tray") }
                .tag(PodcastTab.inbox)

            AgentChatView()
                .tabItem { Label("Agent", systemImage: "sparkles") }
                .tag(PodcastTab.agent)

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
