import SwiftUI

// MARK: - DebugSettingsView
//
// Settings → Debug. Toggles in-memory diagnostic logging and links to the
// log viewer. Logging is off by default and adds overhead while on, so the
// note row spells that out.

struct DebugSettingsView: View {
    @State private var log = DiagnosticLog.shared

    var body: some View {
        Form {
            loggingSection
            viewerSection
        }
        .navigationTitle("Debug")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Sections

    private var loggingSection: some View {
        Section {
            Toggle(isOn: enabledBinding) {
                Label("Enable debug logging", systemImage: "ladybug.fill")
            }
        } header: {
            Text("Logging")
        } footer: {
            Text("Debug logging adds overhead. Disable when not investigating issues.")
        }
    }

    private var viewerSection: some View {
        Section {
            NavigationLink {
                DebugLogsView()
            } label: {
                Label("View logs", systemImage: "doc.text")
            }
        }
    }

    // MARK: - Bindings

    private var enabledBinding: Binding<Bool> {
        Binding(
            get: { log.isEnabled },
            set: { newValue in
                log.isEnabled = newValue
                Haptics.selection()
            }
        )
    }
}
