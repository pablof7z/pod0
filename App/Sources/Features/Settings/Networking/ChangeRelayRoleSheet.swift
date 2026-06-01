import SwiftUI

/// Bottom sheet to change the NIP-65 role of an already-configured relay.
///
/// `set_relay_role` upserts on URL kernel-side, so this dispatches the new
/// canonical role for the existing URL and dismisses; the parent list refreshes
/// from the next `configuredRelays` projection.
struct ChangeRelayRoleSheet: View {
    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    let relay: AppRelayRow
    @State private var role: AppRelayRole

    init(relay: AppRelayRow) {
        self.relay = relay
        // Seed the picker from the relay's current canonical role; an
        // unrecognised composite falls back to the read+write default.
        _role = State(initialValue: AppRelayRole(rawValue: relay.role) ?? .both)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Relay") {
                    Text(relay.url)
                        .font(AppTheme.Typography.monoBody)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .truncationMode(.middle)
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
            .navigationTitle("Change Role")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        store.kernelSetRelayRole(url: relay.url, role: role.wireValue)
                        dismiss()
                    }
                }
            }
            .presentationDetents([.medium])
        }
    }
}

#Preview {
    ChangeRelayRoleSheet(relay: AppRelayRow(url: "wss://relay.example.com", role: "both"))
        .environment(AppStateStore())
}
