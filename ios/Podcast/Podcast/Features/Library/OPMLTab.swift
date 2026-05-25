import SwiftUI
import UniformTypeIdentifiers

// MARK: - OPMLTab

/// "OPML" segment body for `AddShowSheet`. Two flows on one screen:
///
///   - **Import** — file picker (`.opml` / `.xml`) → read text → dispatch
///                  `podcast.import_opml` so the Rust kernel parses the
///                  XML and fans out one `subscribe` per feed URL.
///                  Progress shows up implicitly as the snapshot poll
///                  re-publishes `model.library` while imports land.
///   - **Export** — generate OPML XML client-side from the current
///                  snapshot (`model.library`) and surface a `ShareLink`.
///                  Export is purely presentation — no kernel round-trip.
///
/// All parsing / subscription logic lives in Rust (`podcast-feeds::import_opml`
/// + `host_op_handler::handle_import_opml`). Swift only owns the file picker,
/// the share sheet, and the OPML 2.0 emitter for export. No `AppStateStore`,
/// no `SubscriptionService`.
struct OPMLTab: View {

    let onImported: () -> Void

    @Environment(KernelModel.self) private var model

    @State private var fileImporterShown: Bool = false
    @State private var isImporting: Bool = false
    @State private var importSummary: OPMLImportSummary?
    @State private var importError: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                importSection
                Divider().padding(.vertical, AppTheme.Spacing.xs)
                exportSection
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.top, AppTheme.Spacing.sm)
        }
        .fileImporter(
            isPresented: $fileImporterShown,
            allowedContentTypes: Self.opmlContentTypes,
            allowsMultipleSelection: false
        ) { result in
            handleFileImport(result)
        }
    }

    // MARK: - Import section

    private var importSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text("Import from OPML")
                    .font(AppTheme.Typography.headline)
                Text("Bring your shows over from another podcast app.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
            }

            Button {
                Haptics.light()
                importError = nil
                importSummary = nil
                fileImporterShown = true
            } label: {
                HStack {
                    if isImporting {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "doc.text.below.ecg")
                    }
                    Text(isImporting ? "Importing…" : "Choose OPML file…")
                        .frame(maxWidth: .infinity)
                }
                .padding(.vertical, AppTheme.Spacing.md)
            }
            .buttonStyle(.glassProminent)
            .disabled(isImporting)

            if let importSummary {
                summaryView(importSummary)
            }

            if let importError {
                Label(importError, systemImage: "exclamationmark.triangle.fill")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(AppTheme.Tint.error)
            }
        }
    }

    private func summaryView(_ s: OPMLImportSummary) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Label(
                "\(s.imported) added, \(s.skipped) already subscribed, \(s.errors.count) failed",
                systemImage: "checkmark.circle.fill"
            )
            .font(AppTheme.Typography.subheadline)
            .foregroundStyle(s.errors.isEmpty ? .secondary : AppTheme.Tint.error)

            if !s.errors.isEmpty {
                ForEach(s.errors.prefix(5), id: \.feedURL) { row in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(row.title.isEmpty ? row.feedURL : row.title)
                            .font(AppTheme.Typography.caption.weight(.semibold))
                            .lineLimit(1)
                        Text(row.error)
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(AppTheme.Tint.error)
                            .lineLimit(2)
                    }
                    .padding(AppTheme.Spacing.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                            .fill(AppTheme.Tint.error.opacity(0.08))
                    )
                }
            }
        }
        .padding(AppTheme.Spacing.sm)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    // MARK: - Export section

    private var exportSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text("Export your library")
                    .font(AppTheme.Typography.headline)
                Text("Share an OPML 2.0 file with the \(exportableCount) feeds you're subscribed to.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
            }

            if exportableCount == 0 {
                Text("Subscribe to a show first, then come back to export.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            } else {
                ShareLink(
                    item: OPMLExportFile.from(library: model.library),
                    preview: SharePreview("Subscriptions.opml")
                ) {
                    Label("Share OPML file", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, AppTheme.Spacing.md)
                }
                .buttonStyle(.bordered)
            }
        }
    }

    private var exportableCount: Int {
        model.library.reduce(into: 0) { acc, p in
            if p.feedUrl?.isEmpty == false { acc += 1 }
        }
    }

    // MARK: - Import actions

    private static var opmlContentTypes: [UTType] {
        var types: [UTType] = [.xml]
        if let opml = UTType(filenameExtension: "opml") {
            types.insert(opml, at: 0)
        }
        return types
    }

    private func handleFileImport(_ result: Result<[URL], Error>) {
        switch result {
        case .failure(let error):
            importError = error.localizedDescription
        case .success(let urls):
            guard let url = urls.first else { return }
            isImporting = true
            importError = nil
            importSummary = nil
            Task {
                await readAndDispatch(url: url)
            }
        }
    }

    private func readAndDispatch(url: URL) async {
        let content: String
        do {
            let needsScope = url.startAccessingSecurityScopedResource()
            defer { if needsScope { url.stopAccessingSecurityScopedResource() } }
            let data = try Data(contentsOf: url)
            guard let text = String(data: data, encoding: .utf8) else {
                await MainActor.run {
                    importError = "OPML file isn't valid UTF-8 text."
                    isImporting = false
                }
                return
            }
            content = text
        } catch {
            await MainActor.run {
                importError = "Couldn't read the OPML file: \(error.localizedDescription)"
                isImporting = false
            }
            return
        }

        // Dispatch synchronously through the kernel. The Rust handler does the
        // fan-out internally; we get one accepted/failed envelope back. The
        // detailed per-feed `imported / skipped / errors` payload sits in the
        // kernel-side response — for v1 we surface a summary derived from the
        // library snapshot delta (count of new entries vs. before import).
        let countBefore = model.library.count
        let dispatchResult = model.dispatch(
            namespace: "podcast",
            body: ["op": "import_opml", "content": content]
        )

        switch dispatchResult {
        case .accepted:
            // The actor processes the import inline (it's synchronous from the
            // dispatch perspective — `DispatchHostOp` runs to completion before
            // the next action). By the time we get `.accepted` here, the
            // library count has been updated. Read the delta straight off the
            // snapshot for a sharp summary line.
            let countAfter = model.library.count
            let added = max(0, countAfter - countBefore)
            await MainActor.run {
                importSummary = OPMLImportSummary(
                    imported: added,
                    skipped: 0,
                    errors: []
                )
                isImporting = false
                Haptics.success()
                if added > 0 { onImported() }
            }
        case .failure(let message):
            await MainActor.run {
                importError = message
                isImporting = false
                Haptics.warning()
            }
        }
    }
}

// MARK: - OPMLImportSummary

/// Single-page summary surfaced under the Import button after a run completes.
/// Per-feed errors are decoded out-of-band from the Rust response payload
/// (not wired in v1 — the kernel returns them inline but the iOS dispatch
/// envelope only carries `correlation_id`). For v1 we surface the count
/// delta from `model.library` and rely on `lastErrorToast` for hard failures.
struct OPMLImportSummary: Equatable {
    let imported: Int
    let skipped: Int
    let errors: [OPMLImportRowFailure]
}

/// Per-row failure description. Reserved for a future PR that pipes the Rust
/// response payload back to iOS through a side channel.
struct OPMLImportRowFailure: Equatable {
    let feedURL: String
    let title: String
    let error: String
}
