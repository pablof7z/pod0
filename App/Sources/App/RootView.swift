import CoreSpotlight
import Pod0Core
import SwiftUI

/// The root view of the app. Hosts the main tab bar (hidden), onboarding gate,
/// deep-link routing, and the avatar sidebar.
struct RootView: View {
    @Environment(AppStateStore.self) var store
    @Environment(AgentAskCoordinator.self) var askCoordinator
    @Environment(AgentApprovalCoordinator.self) var approvalCoordinator
    @Environment(WorkflowClient.self) var workflows
    @State var selectedTab: RootTab = .home
    @State var showSettings = false
    @State var showAgentChat = false
    @State var showSidebar = false
    @State var showSearch = false
    @State var agentSession: SharedAgentConversationSession?
    @State var requestedAgentConversationID: ConversationId?
    @State var agentUnseenMessageCount: Int = 0
    @State var spotlightSheet: SpotlightIndexer.DeepLink?
    @State var playbackState = PlaybackState()
    @State var showFullPlayer = false
    @State var playerNavSubscriptionID: UUID?
    @Namespace var playerNamespace

    private let sidebarWidth: CGFloat = 300
    @ViewBuilder
    var body: some View {
        if store.sharedLibraryUnavailableReason != nil {
            SharedCoreUnavailableView()
        } else {
            ZStack(alignment: .leading) {
            tabBar
                .environment(playbackState)
                .offset(x: showSidebar ? sidebarWidth : 0)
                .overlay {
                    if showSidebar {
                        Color.clear
                            .ignoresSafeArea()
                            .contentShape(Rectangle())
                            .onTapGesture {
                                Haptics.selection()
                                withAnimation(AppTheme.Animation.spring) { showSidebar = false }
                            }
                    }
                }
                .task {
                    await workflows.reconcileAndDrain()
                }
                .onAppear { setupPlaybackHandlers() }
                .onChange(of: store.state.settings) { _, new in
                    playbackState.applyPreferences(from: new)
                }
                .sheet(isPresented: $showFullPlayer) {
                    PlayerView(state: playbackState, glassNamespace: playerNamespace)
                        .presentationDetents([.large])
                        .presentationDragIndicator(.visible)
                        .presentationBackgroundInteraction(.disabled)
                }
                .sheet(isPresented: $showSettings) {
                    NavigationStack { SettingsView() }
                }
                .sheet(isPresented: $showAgentChat) {
                    if let session = agentSession {
                        NavigationStack {
                            SharedAgentChatView(
                                session: session,
                                requestedConversationID: requestedAgentConversationID
                            )
                                .toolbar {
                                    ToolbarItem(placement: .topBarLeading) {
                                        Button("Done") {
                                            Haptics.selection()
                                            showAgentChat = false
                                        }
                                    }
                                }
                        }
                        .environment(playbackState)
                    }
                }
                .onChange(of: showAgentChat) { _, _ in
                    agentUnseenMessageCount = agentSession?.messages.count ?? 0
                }
                .sheet(item: Binding(
                    get: { spotlightSheet.map(IdentifiedSpotlightLink.init) },
                    set: { spotlightSheet = $0?.link }
                )) { identified in
                    NavigationStack { spotlightDetailView(for: identified.link) }
                }
                .fullScreenCover(
                    isPresented: Binding(
                        get: { !store.state.settings.hasCompletedOnboarding },
                        set: { _ in }
                    )
                ) {
                    OnboardingView()
                }
                .sheet(isPresented: $showSearch) { searchSheet }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.willEnterForegroundNotification)) { _ in
                    store.sharedLibrary?.ensureNostrSigner()
                    Task { await workflows.reconcileAndDrain() }
                }
                .onReceive(NotificationCenter.default.publisher(for: .askAgentRequested)) { _ in
                    showFullPlayer = false
                    openAgentChat()
                }
                .onReceive(NotificationCenter.default.publisher(for: .openPlayerRequested)) { _ in
                    showFullPlayer = true
                }
                .onReceive(NotificationCenter.default.publisher(for: .openSubscriptionDetailRequested)) { note in
                    guard let idString = note.userInfo?["subscriptionID"] as? String,
                          let uuid = UUID(uuidString: idString) else { return }
                    showFullPlayer = false
                    playerNavSubscriptionID = uuid
                }
                .onReceive(NotificationCenter.default.publisher(for: .openAgentChatConversation)) { note in
                    guard let convID = note.userInfo?["conversationID"] as? UUID else { return }
                    showFullPlayer = false
                    openAgentChat(conversationID: ConversationId(uuid: convID))
                }
                .modifier(PlayerNavSheets(
                    subscriptionID: $playerNavSubscriptionID,
                    store: store
                ))
                .agentAskPresenter(coordinator: askCoordinator)
                .agentApprovalPresenter(coordinator: approvalCoordinator)
                .onOpenURL { handleDeepLink($0) }
                .onReceive(
                    NotificationCenter.default.publisher(for: AppDelegate.shortcutURLNotification)
                ) { note in
                    if let url = note.object as? URL { handleDeepLink(url) }
                }
                .onContinueUserActivity(CSSearchableItemActionType, perform: handleSpotlight)

            AppSidebarView(
                selectedTab: $selectedTab,
                isPresented: $showSidebar,
                onOpenSettings: {
                    showSettings = true
                }
            )
            .frame(width: sidebarWidth)
            .ignoresSafeArea()
            .offset(x: showSidebar ? 0 : -sidebarWidth)
            .zIndex(100)
            }
        }
    }

    // MARK: - Tab bar
    @ViewBuilder
    private var tabBar: some View {
        let base = TabView(selection: $selectedTab) {
            ForEach(RootTab.allCases, id: \.self) { tab in
                tabContent(for: tab)
                    .tabItem { Label(tab.rawValue, systemImage: tab.icon) }
                    .tag(tab)
            }
        }
        .tabBarMinimizeBehavior(.onScrollDown)

        if playbackState.episode != nil {
            base.tabViewBottomAccessory {
                MiniPlayerView(
                    state: playbackState,
                    onTap: { showFullPlayer = true },
                    glassNamespace: playerNamespace
                )
            }
        } else {
            base
        }
    }

    @ViewBuilder
    private func tabContent(for tab: RootTab) -> some View {
        switch tab {
        case .home:
            NavigationStack {
                HomeView()
                    .toolbar { sharedToolbar() }
            }
            .toolbar(.hidden, for: .tabBar)
        case .library:
            NavigationStack {
                AllEpisodesView()
                    .toolbar { sharedToolbar() }
            }
            .toolbar(.hidden, for: .tabBar)
        case .clips:
            NavigationStack {
                ClipsView()
                    .toolbar { sharedToolbar() }
            }
            .toolbar(.hidden, for: .tabBar)
        }
    }

    // MARK: - Search sheet

    private var searchSheet: some View {
        NavigationStack {
            PodcastSearchView()
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Done") {
                            Haptics.selection()
                            showSearch = false
                        }
                    }
                }
        }
        .environment(playbackState)
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private func sharedToolbar() -> some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            let settings = store.state.settings
            let name = settings.agentDisplayName.trimmed
            Button {
                Haptics.selection()
                withAnimation(AppTheme.Animation.spring) { showSidebar = true }
            } label: {
                AvatarView(
                    url: URL(string: settings.agentAvatarURLString.trimmed),
                    initial: name.first,
                    size: 28
                )
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Open sidebar")
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.selection()
                showSearch = true
            } label: {
                Image(systemName: "magnifyingglass")
            }
            .accessibilityLabel("Search")
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.selection()
                openAgentChat()
            } label: {
                Image(systemName: "sparkles")
                    .overlay(alignment: .topTrailing) {
                        if hasUnreadAgentMessages {
                            Circle()
                                .fill(.red)
                                .frame(width: 7, height: 7)
                                .offset(x: 4, y: -4)
                                .transition(.scale.combined(with: .opacity))
                        }
                    }
                    .animation(AppTheme.Animation.springFast, value: hasUnreadAgentMessages)
            }
            .accessibilityLabel(hasUnreadAgentMessages ? "Open Agent — new reply" : "Open Agent")
            .keyboardShortcut("a", modifiers: [.command, .shift])
        }
    }

    // MARK: - Helper types

    private struct IdentifiedSpotlightLink: Identifiable {
        let link: SpotlightIndexer.DeepLink

        var id: String {
            switch link {
            case .note(let id):         return "note:\(id)"
            case .memory(let id):       return "memory:\(id)"
            case .subscription(let id): return "subscription:\(id)"
            case .episode(let id):      return "episode:\(id)"
            }
        }
    }
}
