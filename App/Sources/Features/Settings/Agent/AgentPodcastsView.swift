import SwiftUI

// MARK: - AgentPodcastsView
//
// Lists agent-owned podcasts (shows created via the `create_podcast` tool or
// the AI agent) and lets the user toggle per-podcast Nostr visibility between
// private (library only) and public (published as NIP-74 kind:30074 events).
//
// Also manages the relay list used when publishing NIP-74 events — initialised
// from the user's NIP-65 kind:10002 outbox relays, falling back to
// relay.primal.net and relay.damus.io when none are found.

struct AgentPodcastsView: View {
    @Environment(AppStateStore.self) private var store
    @State private var isAddingRelay = false
    @State private var newRelayURL = ""
    @State private var isFetchingRelays = false

    private var ownedPodcasts: [Podcast] {
        store.allPodcasts.filter { $0.ownerPubkeyHex != nil }
            .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
    }

    private var publicRelays: [String] {
        store.state.settings.nostrPublicRelays
    }

    var body: some View {
        List {
            if ownedPodcasts.isEmpty {
                emptyState
            } else {
                podcastsSection
            }
            relaySection
        }
        .settingsListStyle()
        .navigationTitle("Agent Podcasts")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $isAddingRelay) {
            addRelaySheet
        }
        .task {
            if publicRelays.isEmpty {
                await initializeRelays()
            }
        }
    }

    // MARK: - Sections

    private var podcastsSection: some View {
        Section {
            ForEach(ownedPodcasts) { podcast in
                AgentPodcastRow(podcast: podcast)
            }
        } header: {
            Text("Owned Shows")
        } footer: {
            Text("Public shows are published as NIP-74 Nostr events signed by the agent's key. Episodes added to public shows are also published when Nostr is enabled.")
        }
    }

    private var emptyState: some View {
        Section {
            VStack(spacing: 12) {
                Image(systemName: "waveform.badge.plus")
                    .font(.system(size: 40))
                    .foregroundStyle(.secondary)
                Text("No agent-owned podcasts yet")
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.secondary)
                Text("Ask the agent to create a podcast using the create_podcast tool.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 24)
        }
    }

    private var relaySection: some View {
        Section {
            ForEach(publicRelays, id: \.self) { relay in
                HStack {
                    Image(systemName: "antenna.radiowaves.left.and.right")
                        .foregroundStyle(.secondary)
                        .frame(width: 20)
                    Text(relay)
                        .font(AppTheme.Typography.monoCaption)
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .onDelete { indexSet in
                var relays = publicRelays
                relays.remove(atOffsets: indexSet)
                updateRelays(relays)
            }

            if isFetchingRelays {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Fetching your relay list…")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            } else {
                Button {
                    isAddingRelay = true
                } label: {
                    Label("Add relay", systemImage: "plus.circle.fill")
                        .foregroundStyle(Color.accentColor)
                }
            }
        } header: {
            Label("Publishing Relays", systemImage: "network")
        } footer: {
            Text("NIP-74 podcast events are published to these relays. Initialized from your NIP-65 relay list. Swipe a row to remove.")
        }
    }

    // MARK: - Add relay sheet

    private var addRelaySheet: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("wss://relay.example.com", text: $newRelayURL)
                        .font(AppTheme.Typography.monoCallout)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                } header: {
                    Text("Relay URL")
                } footer: {
                    Text("WebSocket relay URL starting with wss://")
                }
            }
            .navigationTitle("Add Relay")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        newRelayURL = ""
                        isAddingRelay = false
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        let trimmed = newRelayURL.trimmed
                        if !trimmed.isEmpty, !publicRelays.contains(trimmed) {
                            updateRelays(publicRelays + [trimmed])
                        }
                        newRelayURL = ""
                        isAddingRelay = false
                    }
                    .disabled(newRelayURL.trimmed.isEmpty)
                }
            }
        }
        .presentationDetents([.medium])
    }

    // MARK: - Helpers

    private func updateRelays(_ relays: [String]) {
        var settings = store.state.settings
        settings.nostrPublicRelays = relays
        store.updateSettings(settings)
    }

    private func initializeRelays() async {
        let userPubkey = await MainActor.run { UserIdentityStore.shared.publicKeyHex }
        let inboxRelay = await MainActor.run { store.state.settings.nostrRelayURL }

        isFetchingRelays = true
        defer { Task { @MainActor in isFetchingRelays = false } }

        var relays: [String] = []
        if let pubkey = userPubkey, !pubkey.isEmpty {
            relays = await NIP65RelayFetcher.fetchWriteRelays(
                for: pubkey,
                extraRelayURL: inboxRelay.isEmpty ? nil : inboxRelay
            )
        }
        if relays.isEmpty {
            relays = NIP65RelayFetcher.defaultRelays
        }
        await MainActor.run { updateRelays(relays) }
    }
}

// MARK: - AgentPodcastRow

private struct AgentPodcastRow: View {
    @Environment(AppStateStore.self) private var store
    let podcast: Podcast

    private var isPublic: Bool { podcast.nostrVisibility == .public }

    var body: some View {
        HStack(spacing: 12) {
            CachedAsyncImage(url: podcast.imageURL, targetSize: CGSize(width: 44, height: 44)) { phase in
                switch phase {
                case .success(let img):
                    img.resizable().aspectRatio(contentMode: .fill)
                default:
                    RoundedRectangle(cornerRadius: 8).fill(Color(.systemFill))
                        .overlay(Image(systemName: "mic.fill").foregroundStyle(.secondary))
                }
            }
            .frame(width: 44, height: 44)
            .clipShape(RoundedRectangle(cornerRadius: 8))

            VStack(alignment: .leading, spacing: 2) {
                Text(podcast.title)
                    .font(AppTheme.Typography.callout)
                    .lineLimit(1)
                if !podcast.author.isEmpty {
                    Text(podcast.author)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Text(isPublic ? "Public · Nostr" : "Private")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(isPublic ? Color.accentColor : .secondary)
            }

            Spacer()

            Toggle("", isOn: Binding(
                get: { isPublic },
                set: { newValue in
                    var updated = podcast
                    updated.nostrVisibility = newValue ? .public : .private
                    store.updatePodcast(updated)
                    Haptics.selection()
                    if newValue {
                        let podcastID = podcast.id.uuidString
                        Task {
                            let mgr = LiveAgentOwnedPodcastManager(store: store)
                            _ = try? await mgr.updatePodcast(
                                podcastID: podcastID,
                                title: nil, description: nil, author: nil,
                                imageURL: nil, visibility: .public
                            )
                        }
                    }
                }
            ))
            .labelsHidden()
        }
        .accessibilityLabel("\(podcast.title), \(isPublic ? "public" : "private")")
        .accessibilityHint("Toggle Nostr visibility")
    }
}
