import SwiftUI

struct DataStorageSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @State private var storageSummary: String?
    @State private var showClearConfirmation = false
    @State private var isClearing = false
    @State private var clearFailure: String?

    var body: some View {
        List {
            dataSection
            storageSection
            destructiveSection
        }
        .settingsListStyle()
        .navigationTitle("Data & Storage")
        .navigationBarTitleDisplayMode(.inline)
        .task { await refreshStorageSummary() }
        .alert("Clear All Data?", isPresented: $showClearConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Clear All Data", role: .destructive) {
                isClearing = true
                clearFailure = nil
                Task { @MainActor in
                    do {
                        try await store.clearAllDataAndWait()
                        Haptics.success()
                    } catch {
                        clearFailure = "Deletion could not finish safely. Restart Podcastr to resume."
                    }
                    isClearing = false
                }
            }
        } message: {
            Text("This permanently deletes your Podcastr library. API credentials, settings, and cached network data are preserved.")
        }
    }

    private var dataSection: some View {
        Section("Data") {
            NavigationLink {
                DataExportView()
            } label: {
                SettingsRow(
                    icon: "square.and.arrow.up",
                    tint: .teal,
                    title: "Export Data",
                    value: dataRecordCount > 0 ? "\(dataRecordCount) records" : nil
                )
            }
            NavigationLink {
                ProductSignalsView()
            } label: {
                SettingsRow(
                    icon: "waveform.path.ecg",
                    tint: .indigo,
                    title: "Product Signals",
                    value: "Private & local"
                )
            }
        }
    }

    private var storageSection: some View {
        Section("Storage") {
            NavigationLink {
                StorageSettingsView()
            } label: {
                SettingsRow(
                    icon: "internaldrive.fill",
                    tint: .gray,
                    title: "Downloads & Disk",
                    value: storageSummary
                )
            }
        }
    }

    private var destructiveSection: some View {
        Section {
            Button(isClearing ? "Clearing…" : "Clear All Data", role: .destructive) {
                showClearConfirmation = true
            }
            .disabled(isClearing)
        } footer: {
            Text(clearFailure ?? "Deletes Podcastr product data. Credentials and cached network data stay intact.")
        }
    }

    private var dataRecordCount: Int {
        store.state.subscriptions.count
            + store.state.episodes.count
            + store.activeNotes.count
            + store.activeMemories.count
    }

    private func refreshStorageSummary() async {
        let snap = await StorageSettingsView.compute(store: store)
        await MainActor.run {
            storageSummary = snap.totalBytes > 0 ? SettingsView.formatSize(snap.totalBytes) : nil
        }
    }
}
