import SwiftUI

// MARK: - NostrDiscoverForm

/// "Nostr" segment body in `AddShowSheet`. Surfaces NIP-F4 (`kind:10154`)
/// podcast shows discovered by the Rust kernel and lets the user subscribe to
/// the RSS feed each show advertises.
///
/// Fully snapshot-driven: a discovery sweep is dispatched to the kernel on
/// appear (and whenever the configured relay changes). Results arrive on
/// `store.kernel?.podcastSnapshot?.nostrResults` via the reactive push seam and
/// render as the relay responds — there is no local loading state and no
/// spinner. Search filters the projected list client-side; no extra dispatches.
struct NostrDiscoverForm: View {

    let store: AppStateStore
    let onAdded: (Podcast) -> Void

    @State private var query: String = ""
    @State private var subscribingID: String?
    @State private var rowErrors: [String: String] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if configuredRelayURL != nil {
                searchField
                content
            } else {
                nostrNotConfiguredState
            }
        }
        .onAppear { store.kernelDiscoverNostrClaim() }
        .onDisappear { store.kernelDiscoverNostrRelease() }
    }

    // MARK: - Computed state

    private var configuredRelayURL: URL? {
        let settings = store.state.settings
        guard !settings.nostrRelayURL.isEmpty,
              let url = URL(string: settings.nostrRelayURL) else { return nil }
        return url
    }

    /// Live results projected from the kernel snapshot. The push seam keeps this
    /// current — reading it directly means the view re-renders as results land.
    private var results: [NostrShowSummary] {
        store.kernel?.podcastSnapshot?.nostrResults ?? []
    }

    private var filteredShows: [NostrShowSummary] {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !q.isEmpty else { return results }
        return results.filter { show in
            show.title.lowercased().contains(q) ||
            (show.description?.lowercased().contains(q) ?? false) ||
            (show.categories?.contains { $0.lowercased().contains(q) } ?? false)
        }
    }

    // MARK: - Search field

    private var searchField: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Filter shows", text: $query)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .frame(maxWidth: .infinity)
            if !query.isEmpty {
                Button { query = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Clear")
            }
        }
        .padding(AppTheme.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.bottom, AppTheme.Spacing.sm)
    }

    // MARK: - Content switching

    @ViewBuilder
    private var content: some View {
        if filteredShows.isEmpty {
            emptyState
        } else {
            showsList
        }
    }

    /// Empty means either "the relay hasn't responded yet" or "genuinely none".
    /// Without a loading flag we can't distinguish, so the copy stays neutral —
    /// a static label, never an animated spinner. Results replace it the moment
    /// the kernel pushes a non-empty `nostrResults`.
    private var emptyState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 48)
            Image(systemName: "antenna.radiowaves.left.and.right")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.tertiary)
            if query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text("Searching…")
                    .font(AppTheme.Typography.headline)
                Text("Looking for NIP-F4 shows on this relay. Results appear as the relay responds.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            } else {
                Text("No matches")
                    .font(AppTheme.Typography.headline)
                Text("Try a different search term.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    private var nostrNotConfiguredState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 48)
            Image(systemName: "person.2.slash")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.tertiary)
            Text("Nostr not configured")
                .font(AppTheme.Typography.headline)
            Text("Configure a Nostr relay in Settings → Agent to discover podcasts published on Nostr.")
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    // MARK: - Shows list

    private var showsList: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                HStack {
                    Text("Nostr Shows")
                        .font(AppTheme.Typography.headline)
                    Spacer()
                    Text("\(filteredShows.count)")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.bottom, AppTheme.Spacing.xs)

                ForEach(filteredShows) { show in
                    NostrShowRow(
                        show: show,
                        isSubscribing: subscribingID == show.id,
                        isAlreadySubscribed: isAlreadySubscribed(show),
                        rowError: rowErrors[show.id],
                        onSubscribe: { Task { await subscribe(to: show) } }
                    )
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    Divider()
                        .padding(.leading, AppTheme.Spacing.lg + 64 + AppTheme.Spacing.md)
                }
            }
            .padding(.bottom, AppTheme.Spacing.xl)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    // MARK: - Logic

    private func isAlreadySubscribed(_ show: NostrShowSummary) -> Bool {
        guard let feed = show.feedUrl, let url = URL(string: feed),
              let existing = store.podcast(feedURL: url) else { return false }
        return store.subscription(podcastID: existing.id) != nil
    }

    private func subscribe(to show: NostrShowSummary) async {
        guard subscribingID == nil else { return }
        guard let feed = show.feedUrl, !feed.isEmpty else {
            rowErrors[show.id] = "This show has no feed to subscribe to."
            return
        }
        subscribingID = show.id
        rowErrors.removeValue(forKey: show.id)
        defer { subscribingID = nil }

        do {
            let podcast = try await store.kernelSubscribe(feedURL: feed)
            Haptics.success()
            onAdded(podcast)
        } catch {
            rowErrors[show.id] = error.localizedDescription
        }
    }
}

// MARK: - NostrShowRow

private struct NostrShowRow: View {

    let show: NostrShowSummary
    let isSubscribing: Bool
    let isAlreadySubscribed: Bool
    let rowError: String?
    let onSubscribe: () -> Void

    /// A show with no feed URL can't be subscribed (there is no Nostr-native
    /// subscribe op — only the RSS `feed_url` the kind:10154 event carries).
    private var isSubscribable: Bool {
        guard let feed = show.feedUrl else { return false }
        return !feed.isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
                artwork
                VStack(alignment: .leading, spacing: 2) {
                    Text(show.title.isEmpty ? "Untitled show" : show.title)
                        .font(AppTheme.Typography.headline)
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                    if let description = show.description, !description.isEmpty {
                        Text(description)
                            .font(AppTheme.Typography.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                    if let categories = show.categories, !categories.isEmpty {
                        Text(categories.prefix(2).joined(separator: " · "))
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .padding(.top, 2)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                trailingControl
                    .padding(.top, 2)
            }

            if let rowError {
                Text(rowError)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.red)
                    .padding(.leading, 64 + AppTheme.Spacing.md)
            }
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture {
            guard !isSubscribing, !isAlreadySubscribed, isSubscribable else { return }
            onSubscribe()
        }
        .opacity(isSubscribing || isAlreadySubscribed || !isSubscribable ? 0.65 : 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
    }

    private var artwork: some View {
        CachedAsyncImage(url: show.artworkUrl.flatMap(URL.init(string:)),
                         targetSize: CGSize(width: 64, height: 64)) { phase in
            switch phase {
            case .success(let image):
                image.resizable().aspectRatio(contentMode: .fill)
            case .empty, .failure:
                ZStack {
                    Color(.tertiarySystemFill)
                    Image(systemName: "antenna.radiowaves.left.and.right")
                        .foregroundStyle(.secondary)
                }
            @unknown default:
                Color(.tertiarySystemFill)
            }
        }
        .frame(width: 64, height: 64)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
    }

    @ViewBuilder
    private var trailingControl: some View {
        if isSubscribing {
            ProgressView().controlSize(.small).frame(width: 32, height: 32)
        } else if isAlreadySubscribed {
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 32, height: 32)
        } else if isSubscribable {
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        } else {
            Image(systemName: "link.badge.plus")
                .font(.title3)
                .foregroundStyle(.tertiary)
                .frame(width: 32, height: 32)
        }
    }

    private var accessibilityLabel: String {
        if isAlreadySubscribed { return "Already subscribed to \(show.title)" }
        if isSubscribing { return "Subscribing to \(show.title)" }
        if !isSubscribable { return "\(show.title) has no feed to subscribe to" }
        return "Subscribe to \(show.title) on Nostr"
    }
}
