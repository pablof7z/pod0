import SwiftUI

// MARK: - FeedbackView

struct FeedbackView: View {
    @Bindable var workflow: FeedbackWorkflow
    @Environment(\.dismiss) private var dismiss
    @Environment(UserIdentityStore.self) private var userIdentity
    @Environment(AppStateStore.self) private var appStore

    @State private var store = FeedbackStore()
    @State private var composerPresented = false
    @State private var voiceSheetPresented = false
    @State private var showMine = true
    @State private var searchText = ""

    private var visibleThreads: [FeedbackThread] {
        guard !searchText.isBlank else {
            return segmentFilteredThreads
        }
        // Locale-aware fold: matches "Straße" against "STRASSE", "İstanbul"
        // against "istanbul", and avoids the four `.lowercased()`
        // allocations per row the previous shape did per render.
        return segmentFilteredThreads.filter { thread in
            (thread.title ?? "").localizedCaseInsensitiveContains(searchText)
            || thread.content.localizedCaseInsensitiveContains(searchText)
            || (thread.summary ?? "").localizedCaseInsensitiveContains(searchText)
            || thread.category.rawValue.localizedCaseInsensitiveContains(searchText)
        }
    }

    private var segmentFilteredThreads: [FeedbackThread] {
        guard showMine, let pubkey = userIdentity.publicKeyHex else { return store.threads }
        return store.threads.filter { $0.authorPubkey == pubkey }
    }

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("Feedback")
                .navigationBarTitleDisplayMode(.inline)
                .searchable(text: $searchText, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search feedback")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { dismiss() }
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        trailingToolbarButtons
                    }
                }
        }
        .task {
            await store.load(identity: userIdentity)
            // First load: if "Everyone" was selected last time, fetch
            // profiles so author names land before the row renders.
            if !showMine { await fetchVisibleProfiles() }
        }
        .sheet(isPresented: $composerPresented) {
            FeedbackComposeView(store: store, workflow: workflow)
        }
        .sheet(isPresented: $voiceSheetPresented) {
            FeedbackVoiceRecordingSheet(store: store, workflow: workflow)
        }
        .onAppear {
            if workflow.screenshot != nil || workflow.annotatedImage != nil {
                composerPresented = true
            }
        }
        .onChange(of: showMine) { _, isMine in
            guard !isMine else { return }
            Task { await fetchVisibleProfiles() }
        }
    }

    private func fetchVisibleProfiles() async {
        let others = store.threads
            .map(\.authorPubkey)
            .filter { !$0.isEmpty && $0 != userIdentity.publicKeyHex }
        guard !others.isEmpty else { return }
        await NostrProfileFetcher(store: appStore).fetchProfiles(for: Array(Set(others)))
    }

    private func authorName(for thread: FeedbackThread) -> String? {
        guard !showMine,
              thread.authorPubkey != userIdentity.publicKeyHex,
              !thread.authorPubkey.isEmpty
        else { return nil }
        return appStore.state.nostrProfileCache[thread.authorPubkey]?.bestLabel
            ?? String(thread.authorPubkey.prefix(8))
    }

    // MARK: - Trailing toolbar

    @ViewBuilder
    private var trailingToolbarButtons: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            NavigationLink {
                IdentityRootView()
            } label: {
                Image(systemName: "person.crop.circle")
            }
            .accessibilityLabel("Identity")

            Button {
                Haptics.selection()
                voiceSheetPresented = true
            } label: {
                Image(systemName: "mic")
            }
            .accessibilityLabel("Record feedback")

            Button {
                composerPresented = true
            } label: {
                Image(systemName: "square.and.pencil")
            }
            .accessibilityLabel("New feedback")
        }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        if store.isLoading && store.threads.isEmpty {
            loadingSkeleton
        } else if let loadError = store.loadError, store.threads.isEmpty {
            ContentUnavailableView(
                "Feedback unavailable",
                systemImage: "wifi.exclamationmark",
                description: Text(loadError)
            )
        } else if store.threads.isEmpty {
            emptyState
        } else if visibleThreads.isEmpty {
            noSearchResults
        } else {
            threadList
        }
    }

    // MARK: - Thread list

    @ViewBuilder
    private var threadList: some View {
        List {
            mineEveryoneSegmentedControl
                .listRowInsets(AppTheme.Layout.cardRowInsetsSM)

            ForEach(visibleThreads) { thread in
                NavigationLink {
                    FeedbackThreadDetailView(thread: thread, store: store)
                        .task { await store.loadReplies(for: thread, identity: userIdentity) }
                } label: {
                    FeedbackThreadRow(
                        thread: thread,
                        query: searchText,
                        authorName: authorName(for: thread)
                    )
                }
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    Button(role: .destructive) {
                        Haptics.warning()
                        store.deleteThread(id: thread.id)
                    } label: {
                        Label("Delete", systemImage: "trash")
                    }
                }
            }
        }
        .listStyle(.plain)
        .refreshable { await store.load(identity: userIdentity) }
    }

    // MARK: - Segmented control

    @ViewBuilder
    private var mineEveryoneSegmentedControl: some View {
        Picker("Show", selection: $showMine) {
            Text("Mine").tag(true)
            Text("Everyone").tag(false)
        }
        .pickerStyle(.segmented)
        .padding(.vertical, 4)
    }

    // MARK: - Loading skeleton

    @ViewBuilder
    private var loadingSkeleton: some View {
        List {
            ForEach(0..<3, id: \.self) { _ in
                FeedbackThreadRow(thread: Self.placeholderThread)
                    .redacted(reason: .placeholder)
            }
        }
        .listStyle(.plain)
    }

    private static var placeholderThread: FeedbackThread {
        FeedbackThread(
            category: .bug,
            content: "This is a placeholder feedback item for skeleton loading state.",
            title: "Placeholder thread title here",
            summary: "Short summary of the thread for preview purposes."
        )
    }

    // MARK: - Empty state

    @ViewBuilder
    private var emptyState: some View {
        ContentUnavailableView {
            Label("No feedback yet", systemImage: "bubble.left.and.bubble.right")
        } description: {
            Text("Tap the pencil to share your thoughts.")
        }
    }

    // MARK: - No search results

    @ViewBuilder
    private var noSearchResults: some View {
        ContentUnavailableView.search(text: searchText)
    }

}
