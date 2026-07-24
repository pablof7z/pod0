import SwiftUI

struct AgentSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @State private var settings: Settings = Settings()

    var body: some View {
        List {
            agentSection
        }
        .settingsListStyle()
        .navigationTitle("Agent")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            settings = store.state.settings
        }
        .onChange(of: settings) { _, new in
            store.updateSettings(new)
        }
    }

    // MARK: - Sections

    private var agentSection: some View {
        Section("Agent") {
            NavigationLink {
                AgentMemoriesView()
            } label: {
                SettingsRow(
                    icon: "brain",
                    tint: .purple,
                    title: "Memories",
                    badge: store.activeMemories.count
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

        }
    }
}
