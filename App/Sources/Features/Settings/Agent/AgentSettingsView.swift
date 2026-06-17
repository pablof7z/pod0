import SwiftUI

struct AgentSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @ObservedObject private var runLogger = AgentRunLogger.shared
    @State private var settings: Settings = Settings()
    @State private var hasNostrKey: Bool = false
    @State private var showAgentRelaySettings = false

    var body: some View {
        List {
            agentSection
            nostrSection
        }
        .settingsListStyle()
        .navigationTitle("Agent")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            settings = store.state.settings
            // Identity readiness now comes from the kernel-backed identity
            // store (the kernel owns the key); NostrCredentialStore is no
            // longer the source of truth.
            hasNostrKey = store.identity.hasIdentity
        }
        .onChange(of: settings) { _, new in
            store.updateSettings(new)
        }
        .onChange(of: settings.nostrEnabled) { Haptics.selection() }
        .sheet(isPresented: $showAgentRelaySettings) {
            AgentConnectionSettingsView(
                relayURL: $settings.nostrRelayURL,
                hasPrivateKey: hasNostrKey
            )
        }
    }

    private var agentRelayRow: some View {
        Button {
            showAgentRelaySettings = true
        } label: {
            HStack {
                SettingsRow(
                    icon: "antenna.radiowaves.left.and.right",
                    tint: .mint,
                    title: "Agent Relay"
                )
                Spacer(minLength: 12)
                Text(agentRelayDisplay)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Agent Relay")
        .accessibilityValue(agentRelayDisplay)
    }

    private var agentRelayDisplay: String {
        let url = settings.nostrRelayURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !url.isEmpty else { return "Not set" }
        return URL(string: url)?.host ?? url
    }

    // MARK: - Sections

    private var agentSection: some View {
        Section("Agent") {
            NavigationLink {
                AgentIdentityView()
            } label: {
                SettingsRow(
                    icon: "person.crop.circle",
                    tint: .pink,
                    title: "Identity"
                )
            }

            NavigationLink {
                AgentFriendsView()
            } label: {
                SettingsRow(
                    icon: "person.2.fill",
                    tint: .blue,
                    title: "Friends",
                    badge: store.state.friends.count
                )
            }

            NavigationLink {
                AgentAccessControlView()
            } label: {
                SettingsRow(
                    icon: "checkmark.shield.fill",
                    tint: .green,
                    title: "Access Control"
                )
            }

            NavigationLink {
                NostrConversationsView()
            } label: {
                SettingsRow(
                    icon: "bubble.left.and.bubble.right.fill",
                    tint: .cyan,
                    title: "Conversations",
                    badge: store.state.nostrConversations.count
                )
            }

            NavigationLink {
                AgentMemoriesView()
            } label: {
                SettingsRow(
                    icon: "brain",
                    tint: .purple,
                    title: "Memories",
                    badge: store.kernel?.podcastSnapshot?.memoryFacts.count ?? 0
                )
            }

            NavigationLink {
                AgentNotesView()
            } label: {
                SettingsRow(
                    icon: "note.text",
                    tint: .indigo,
                    title: "Notes",
                    badge: store.activeNotes.count
                )
            }

            NavigationLink {
                AgentScheduledTasksView()
            } label: {
                SettingsRow(
                    icon: "calendar.badge.clock",
                    tint: .teal,
                    title: "Tasks",
                    badge: store.scheduledTasks.count
                )
            }

            NavigationLink {
                AgentRunListView()
            } label: {
                SettingsRow(
                    icon: "doc.text.magnifyingglass",
                    tint: .orange,
                    title: "Run History",
                    badge: runLogger.runs.count
                )
            }

            NavigationLink {
                AgentActivityLogView()
            } label: {
                SettingsRow(
                    icon: "clock.arrow.circlepath",
                    tint: .teal,
                    title: "Activity Log",
                    badge: store.activeAgentActivityCount
                )
            }

            NavigationLink {
                AgentPodcastsView()
            } label: {
                SettingsRow(
                    icon: "mic.fill",
                    tint: .red,
                    title: "Podcasts",
                    badge: store.rustOwnedPodcasts().count
                )
            }
        }
    }

    private var nostrSection: some View {
        Section {
            Toggle("Enabled", isOn: $settings.nostrEnabled)
                .disabled(!hasNostrKey)

            agentRelayRow

            if !hasNostrKey {
                NavigationLink {
                    AgentIdentityView()
                } label: {
                    Label("Set up identity first", systemImage: "person.crop.circle.badge.exclamationmark")
                        .foregroundStyle(.secondary)
                        .font(AppTheme.Typography.callout)
                }
            }
        } header: {
            Text("Nostr")
        } footer: {
            if hasNostrKey {
                Text("When enabled, this agent can receive and respond to messages over the Nostr network.")
            } else {
                Text("Generate a Nostr key pair in Identity before enabling this feature.")
            }
        }
    }
}
