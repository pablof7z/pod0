import SwiftUI

/// Sheet to add a new app relay: a URL field plus a NIP-65 role picker.
///
/// On "Add" the URL is validated to start with `wss://` or `ws://`, then the
/// canonical role string is dispatched via `kernelAddRelay`. Reactivity is
/// Rust-side, so the sheet simply dismisses and the parent list refreshes from
/// the next `configuredRelays` projection.
struct AddRelaySheet: View {
    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    @State private var url: String = ""
    @State private var role: AppRelayRole = .addDefault
    @State private var validationError: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("wss://relay.example.com", text: $url)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .font(AppTheme.Typography.monoBody)
                } header: {
                    Text("Relay URL")
                } footer: {
                    if let validationError {
                        Text(validationError).foregroundStyle(.red)
                    } else {
                        Text("Must start with wss:// or ws://")
                    }
                }

                Section("Role") {
                    Picker("Role", selection: $role) {
                        ForEach(AppRelayRole.allCases) { role in
                            Text(role.label).tag(role)
                        }
                    }
                    .pickerStyle(.inline)
                    .labelsHidden()
                }
            }
            .navigationTitle("Add Relay")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") { add() }
                        .disabled(trimmedURL.isEmpty)
                }
            }
        }
    }

    private var trimmedURL: String {
        url.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func add() {
        let candidate = trimmedURL
        guard candidate.hasPrefix("wss://") || candidate.hasPrefix("ws://") else {
            validationError = "Relay URL must start with wss:// or ws://"
            return
        }
        store.kernelAddRelay(url: candidate, role: role.wireValue)
        dismiss()
    }
}

#Preview {
    AddRelaySheet()
        .environment(AppStateStore())
}
