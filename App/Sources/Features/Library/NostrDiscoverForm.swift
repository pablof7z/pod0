import SwiftUI

// MARK: - NostrDiscoverForm

/// "Nostr" segment body in `AddShowSheet`. Fetches kind:30074 podcast shows
/// from the configured relay and lets the user subscribe to them.
///
/// Shows are fetched once on appearance and cached for the session.
/// Search filters the cached list client-side — no extra round-trips.
struct NostrDiscoverForm: View {

    let store: AppStateStore
    let onAdded: (Podcast) -> Void

    @State private var service = NostrPodcastDiscoveryService()
    @State private var shows: [NostrPodcastDiscoveryService.ShowResult] = []
    @State private var query: String = ""
    @State private var isLoading = false
    @State private var loadError: String?
    @State private var subscribingID: String?
    @State private var rowErrors: [String: String] = [:]
    @State private var loaded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let relayURL = configuredRelayURL {
                searchField
                content(relayURL: relayURL)
            } else {
                nostrNotConfiguredState
            }
        }
        .onAppear {
            guard !loaded, let url = configuredRelayURL else { return }
            loaded = true
            Task { await loadShows(relayURL: url) }
        }
    }

    // MARK: - Computed state

    private var configuredRelayURL: URL? {
        let settings = store.state.settings
        guard !settings.nostrRelayURL.isEmpty,
              let url = URL(string: settings.nostrRelayURL) else { return nil }
        return url
    }

    private var filteredShows: [NostrPodcastDiscoveryService.ShowResult] {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !q.isEmpty else { return shows }
        return shows.filter {
            $0.title.lowercased().contains(q) ||
            $0.author.lowercased().contains(q) ||
            $0.description.lowercased().contains(q)
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
    private func content(relayURL: URL) -> some View {
        if isLoading {
            loadingState
        } else if let loadError {
            errorState(loadError, relayURL: relayURL)
        } else if filteredShows.isEmpty {
            emptyState
        } else {
            showsList(relayURL: relayURL)
        }
    }

    private var loadingState: some View {
        VStack {
            Spacer(minLength: 60)
            ProgressView()
            Text("Fetching from relay…")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .padding(.top, AppTheme.Spacing.sm)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private func errorState(_ message: String, relayURL: URL) -> some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 48)
            Image(systemName: "antenna.radiowaves.left.and.right.slash")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.tertiary)
            Text("Couldn't reach relay")
                .font(AppTheme.Typography.headline)
            Text(message)
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button("Retry") {
                Task { await loadShows(relayURL: relayURL) }
            }
            .buttonStyle(.glassProminent)
            .padding(.horizontal, AppTheme.Spacing.xl)
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    private var emptyState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 48)
            Image(systemName: "antenna.radiowaves.left.and.right")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.tertiary)
            if query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text("No Nostr podcasts found")
                    .font(AppTheme.Typography.headline)
                Text("No NIP-74 shows were found on this relay.")
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

    private func showsList(relayURL: URL) -> some View {
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
                        onSubscribe: { Task { await subscribe(to: show, relayURL: relayURL) } }
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

    private func isAlreadySubscribed(_ show: NostrPodcastDiscoveryService.ShowResult) -> Bool {
        let pid = NostrPodcastDiscoveryService.podcastID(for: show.coordinate)
        return store.subscription(podcastID: pid) != nil
    }

    private func loadShows(relayURL: URL) async {
        isLoading = true
        loadError = nil
        defer { isLoading = false }
        shows = await service.fetchShows(relayURL: relayURL)
        if shows.isEmpty && !query.isEmpty { query = "" }
    }

    private func subscribe(
        to show: NostrPodcastDiscoveryService.ShowResult,
        relayURL: URL
    ) async {
        guard subscribingID == nil else { return }
        subscribingID = show.id
        rowErrors.removeValue(forKey: show.id)
        defer { subscribingID = nil }

        let podcast = await service.subscribe(to: show, store: store, relayURL: relayURL)
        Haptics.success()
        onAdded(podcast)
    }
}

// MARK: - NostrShowRow

private struct NostrShowRow: View {

    let show: NostrPodcastDiscoveryService.ShowResult
    let isSubscribing: Bool
    let isAlreadySubscribed: Bool
    let rowError: String?
    let onSubscribe: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: 2) {
                Text(show.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                if !show.author.isEmpty {
                    Text(show.author)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                if !show.categories.isEmpty {
                    Text(show.categories.prefix(2).joined(separator: " · "))
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
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture {
            guard !isSubscribing, !isAlreadySubscribed else { return }
            onSubscribe()
        }
        .opacity(isSubscribing || isAlreadySubscribed ? 0.65 : 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
    }

    private var artwork: some View {
        CachedAsyncImage(url: show.imageURL, targetSize: CGSize(width: 64, height: 64)) { phase in
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
        } else {
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        }
    }

    private var accessibilityLabel: String {
        if isAlreadySubscribed { return "Already subscribed to \(show.title)" }
        if isSubscribing { return "Subscribing to \(show.title)" }
        return "Subscribe to \(show.title) on Nostr"
    }
}
