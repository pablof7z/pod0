import SwiftUI

struct NetworkingSettingsView: View {
    @Environment(AppStateStore.self) private var store

    var body: some View {
        List {
            summarySection
            appRelaysSection
            relaysSection
            rolesSection
        }
        .settingsListStyle()
        .navigationTitle("Networking")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var configuredRelayCount: Int {
        store.kernel?.podcastSnapshot?.configuredRelays.count ?? 0
    }

    private var summarySection: some View {
        Section("Summary") {
            summaryRow("App relays", value: "\(configuredRelayCount)")
            summaryRow("Nostr", value: store.state.settings.nostrEnabled ? "Enabled" : "Disabled")
        }
    }

    private var appRelaysSection: some View {
        Section {
            NavigationLink {
                AppRelaysView()
            } label: {
                HStack {
                    Label("App Relays", systemImage: "network")
                    Spacer(minLength: 12)
                    Text("\(configuredRelayCount)")
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
        } footer: {
            Text("Per-relay NIP-65 roles (read / write / indexer) used for Nostr discovery and publishing.")
        }
    }

    private var relaysSection: some View {
        Section {
            if store.state.settings.nostrRelayURL.isEmpty || !store.state.settings.nostrEnabled {
                ContentUnavailableView(
                    "No relays",
                    systemImage: "network.slash",
                    description: Text("Nostr networking has not been configured yet.")
                )
            } else {
                NavigationLink {
                    RelayDetailView(relayURL: store.state.settings.nostrRelayURL)
                } label: {
                    RelayRowView(relayURL: store.state.settings.nostrRelayURL)
                }
            }
        } header: {
            Text("Agent Relay")
        } footer: {
            Text("The agent relay is used for agent inbox delivery, profile publishing, podcast pairing, and NIP-F4 discovery.")
        }
    }

    private var rolesSection: some View {
        Section("Roles") {
            ForEach(RelayRoleViewModel.allCases) { role in
                HStack(spacing: 10) {
                    Image(systemName: role.icon)
                        .foregroundStyle(Color.accentColor)
                        .frame(width: 22)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(role.rawValue)
                        Text(role.description)
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 2)
            }
        }
    }

    private func summaryRow(_ label: String, value: String) -> some View {
        HStack {
            Text(label)
            Spacer(minLength: 12)
            Text(value)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
    }
}

#Preview {
    NavigationStack {
        NetworkingSettingsView()
            .environment(AppStateStore())
    }
}
