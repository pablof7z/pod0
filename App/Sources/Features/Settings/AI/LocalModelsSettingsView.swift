import SwiftUI

struct LocalModelsSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @State private var specs: [LocalModelSpec] = []
    @State private var isLoading = false
    @State private var catalogError: String?

    var body: some View {
        ZStack {
            Color(.systemGroupedBackground)
                .ignoresSafeArea()

            List {
                modelsSection
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .refreshable { await loadCatalog() }
        }
        .navigationTitle("Local")
        .navigationBarTitleDisplayMode(.inline)
        .task { await loadCatalog() }
    }

    // MARK: - Sections

    private var modelsSection: some View {
        Section {
            if specs.isEmpty {
                catalogStatusRow
            } else {
                ForEach(specs, id: \.id) { spec in
                    // Download state comes from the unified kernel download snapshot
                    // (in-flight) + disk (downloaded). No bespoke manager.
                    LocalModelRowView(
                        spec: spec,
                        state: store.localModelState(for: spec),
                        onDownload: {
                            store.kernelDownloadLocalModel(
                                modelID: spec.id, url: spec.downloadURL.absoluteString)
                        },
                        onCancel: { store.kernelCancelLocalModelDownload(modelID: spec.id) },
                        onDelete: {
                            try? FileManager.default.removeItem(
                                at: DownloadCapability.localModelFileURL(for: spec.id))
                        }
                    )
                }
            }
        } header: {
            Text("Available Models")
        } footer: {
            VStack(alignment: .leading, spacing: 8) {
                Text("Models run fully on-device with no internet connection required. Large downloads (~2.6 GB) — Wi-Fi recommended. Once downloaded, a model becomes selectable under Models for each role, alongside your cloud providers.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var catalogStatusRow: some View {
        if isLoading {
            HStack(spacing: 12) {
                ProgressView()
                Text("Loading models")
                    .foregroundStyle(.secondary)
            }
        } else {
            VStack(alignment: .leading, spacing: 10) {
                Label(
                    catalogError ?? "No local models are available.",
                    systemImage: "exclamationmark.triangle"
                )
                .foregroundStyle(.secondary)

                Button {
                    Task { await loadCatalog() }
                } label: {
                    Label("Try Again", systemImage: "arrow.clockwise")
                }
            }
            .padding(.vertical, 6)
        }
    }

    @MainActor
    private func loadCatalog() async {
        isLoading = true
        catalogError = nil
        defer { isLoading = false }
        switch await LocalModelCatalog.fetch() {
        case .loaded(let loadedSpecs):
            specs = loadedSpecs
        case .failed(let error):
            catalogError = error.localizedDescription
        }
    }
}

#Preview {
    NavigationStack {
        LocalModelsSettingsView()
            .environment(AppStateStore())
    }
}
