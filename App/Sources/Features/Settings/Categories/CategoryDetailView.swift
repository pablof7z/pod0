import SwiftUI

// MARK: - CategoryDetailView
//
// Settings → Categories → <one category>. Rust owns runtime category policy.
// Until Rust has durable category-level auto-download/RAG/notification policy,
// this screen exposes only the category control that is wired to Rust today:
// transcription fan-out to the member podcasts.

struct CategoryDetailView: View {
    @Environment(AppStateStore.self) private var store
    let categoryID: UUID

    var body: some View {
        Form {
            if let category = store.state.categories.first(where: { $0.id == categoryID }) {
                if !category.description.isEmpty {
                    descriptionSection(category)
                }
                featuresSection(for: category)
                subscriptionsSection(category)
            } else {
                missingCategorySection
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Sections

    private func descriptionSection(_ category: PodcastCategory) -> some View {
        Section {
            Text(category.description)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func featuresSection(for category: PodcastCategory) -> some View {
        Section {
            Toggle(isOn: transcriptionToggleBinding(for: category)) {
                Label("Transcription", systemImage: "captions.bubble.fill")
            }
        } header: {
            Text("Features")
        } footer: {
            Text("Disable transcription for categories you don't want analysed — entertainment shows, for example, often don't need transcripts.")
        }
    }

    @ViewBuilder
    private func subscriptionsSection(_ category: PodcastCategory) -> some View {
        let pods = podcasts(in: category)
        Section {
            if pods.isEmpty {
                Text("No subscriptions assigned yet.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(pods) { pod in
                    HStack(spacing: AppTheme.Spacing.sm) {
                        Text(pod.title.isEmpty ? (pod.feedURL?.host ?? "Untitled") : pod.title)
                            .font(AppTheme.Typography.body)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        if !pod.author.isEmpty {
                            Text(pod.author)
                                .font(AppTheme.Typography.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }
            }
        } header: {
            Text(pods.count == 1 ? "1 show" : "\(pods.count) shows")
        }
    }

    private var missingCategorySection: some View {
        Section {
            Text("This category is no longer available. It may have been removed by the categorization service.")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Bindings

    private func transcriptionToggleBinding(for category: PodcastCategory) -> Binding<Bool> {
        Binding(
            get: {
                CategoryLibraryProjection
                    .load(categories: [category], store: self.store)
                    .allTranscriptionEnabled(in: category.id)
                    ?? true
            },
            set: { newValue in
                // Fan out to Rust per-podcast policy. Swift does not persist
                // category-level behavior; the category row reads the kernel
                // projection after the mutation.
                for podcast in podcasts(in: category) {
                    self.store.kernel?.dispatch(namespace: "podcast",
                        body: [
                            "op": "set_podcast_transcription_enabled",
                            "podcast_id": podcast.id.uuidString.lowercased(),
                            "enabled": newValue,
                        ])
                }
                Haptics.selection()
            }
        )
    }

    // MARK: - Derived

    private var navigationTitle: String {
        store.state.categories.first(where: { $0.id == categoryID })?.name ?? "Category"
    }

    /// Resolves a category's podcast IDs to live `Podcast` rows, dropping
    /// entries the user may have unsubscribed from since the categorizer
    /// last ran. Sorted by title to match every other management surface.
    private func podcasts(in category: PodcastCategory) -> [Podcast] {
        CategoryLibraryProjection
            .load(categories: [category], store: store)
            .podcasts(in: category.id, store: store)
    }
}
