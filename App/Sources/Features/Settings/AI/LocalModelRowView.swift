import SwiftUI

struct LocalModelRowView: View {
    let spec: LocalModelSpec
    let state: LocalModelState
    let onDownload: () -> Void
    let onCancel: () -> Void
    let onDelete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(spec.displayName)
                        .font(.headline)
                    Text(formattedSize)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer()

                stateView
            }

            if case .downloading(let progress) = state {
                ProgressView(value: progress)
                    .tint(.blue)
            }
        }
        .padding(.vertical, 8)
    }

    // MARK: - View Helpers

    private var formattedSize: String {
        ByteCountFormatter.string(fromByteCount: spec.sizeBytes, countStyle: .file)
    }

    @ViewBuilder
    private var stateView: some View {
        switch state {
        case .notDownloaded:
            Button(action: onDownload) {
                Text("Download")
                    .font(.subheadline)
                    .fontWeight(.semibold)
            }
            .buttonStyle(.bordered)

        case .downloading:
            Button(action: onCancel) {
                Text("Cancel")
                    .font(.subheadline)
                    .fontWeight(.semibold)
            }
            .buttonStyle(.bordered)
            .foregroundStyle(.red)

        case .downloaded:
            downloadedControls(inUse: false)

        case .active:
            downloadedControls(inUse: true)
        }
    }

    /// Controls for a model whose weights are on disk. Selection happens
    /// per-role under Settings > Models, so this page only downloads, reports
    /// whether the model is currently loaded ("In use"), and deletes.
    @ViewBuilder
    private func downloadedControls(inUse: Bool) -> some View {
        HStack(spacing: 8) {
            if inUse {
                Label("In use", systemImage: "checkmark.circle.fill")
                    .font(.subheadline)
                    .fontWeight(.semibold)
                    .foregroundStyle(.green)
                    .labelStyle(.titleAndIcon)
            } else {
                Text("Downloaded")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            Menu {
                Button("Delete", role: .destructive, action: onDelete)
            } label: {
                Image(systemName: "ellipsis.circle")
                    .foregroundStyle(.secondary)
            }
        }
    }
}

#Preview {
    VStack {
        LocalModelRowView(spec: LocalModelCatalog.all[0], state: .notDownloaded,
                          onDownload: {}, onCancel: {}, onDelete: {})
        LocalModelRowView(spec: LocalModelCatalog.all[0], state: .downloading(progress: 0.45),
                          onDownload: {}, onCancel: {}, onDelete: {})
        LocalModelRowView(spec: LocalModelCatalog.all[0], state: .downloaded,
                          onDownload: {}, onCancel: {}, onDelete: {})
        LocalModelRowView(spec: LocalModelCatalog.all[0], state: .active,
                          onDownload: {}, onCancel: {}, onDelete: {})
    }
    .environment(AppStateStore())
}
