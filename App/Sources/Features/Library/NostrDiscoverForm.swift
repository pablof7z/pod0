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
    @State private var activeSearchSessionID: String?
    @State private var searchError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            searchField
            content
        }
        .onAppear { store.kernelDiscoverNostrClaim() }
        .onDisappear { store.kernelDiscoverNostrRelease() }
    }

    // MARK: - Computed state

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

    private var searchHits: [NostrSearchHit] {
        guard let activeSearchSessionID else { return [] }
        return store.kernel?.nostrSearchSessions[activeSearchSessionID]?.hits ?? []
    }

    private var hasSearchSession: Bool {
        activeSearchSessionID != nil
    }

    // MARK: - Search field

    private var searchField: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Filter shows", text: $query)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
                .onSubmit(submitSearch)
                .frame(maxWidth: .infinity)
            if !query.isEmpty {
                Button { clearSearch() } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Clear")
            }
            Button { submitSearch() } label: {
                Image(systemName: "arrow.right.circle.fill")
                    .foregroundStyle(query.trimmed.isEmpty ? Color.secondary : Color.accentColor)
            }
            .buttonStyle(.plain)
            .disabled(query.trimmed.isEmpty)
            .accessibilityLabel("Search Nostr")
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
        if searchHits.isEmpty && filteredShows.isEmpty {
            emptyState
        } else {
            resultList
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
            if let searchError {
                Text("Search unavailable")
                    .font(AppTheme.Typography.headline)
                Text(searchError)
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            } else if hasSearchSession {
                Text("Searching relays…")
                    .font(AppTheme.Typography.headline)
                Text("Relay results appear as NIP-50 search responses arrive.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            } else if query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
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

    // MARK: - Shows list

    private var resultList: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if hasSearchSession {
                    searchSection
                }
                if !filteredShows.isEmpty {
                    showsSection
                }
            }
            .padding(.bottom, AppTheme.Spacing.xl)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    private var searchSection: some View {
        VStack(spacing: 0) {
            sectionHeader(title: "Relay Search", count: searchHits.count)
            if searchHits.isEmpty {
                Text("No relay results yet.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    .padding(.vertical, AppTheme.Spacing.md)
            } else {
                ForEach(searchHits) { hit in
                    NostrSearchHitRow(
                        hit: hit,
                        isSubscribing: subscribingID == hit.id,
                        isAlreadySubscribed: store.rustIsAlreadySubscribed(
                            feedURL: nil,
                            ownerPubkey: hit.author
                        ),
                        rowError: rowErrors[hit.id],
                        onSubscribe: { Task { await subscribe(to: hit) } }
                    )
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    Divider()
                        .padding(.leading, AppTheme.Spacing.lg + 48 + AppTheme.Spacing.md)
                }
            }
        }
    }

    private var showsSection: some View {
        VStack(spacing: 0) {
            sectionHeader(title: "Nostr Shows", count: filteredShows.count)
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
        .padding(.top, hasSearchSession ? AppTheme.Spacing.lg : 0)
    }

    private func sectionHeader(title: String, count: Int) -> some View {
        HStack {
            Text(title)
                .font(AppTheme.Typography.headline)
            Spacer()
            Text("\(count)")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.bottom, AppTheme.Spacing.xs)
    }

    // MARK: - Logic

    private func clearSearch() {
        query = ""
        activeSearchSessionID = nil
        searchError = nil
    }

    private func submitSearch() {
        let trimmed = query.trimmed
        guard !trimmed.isEmpty else {
            clearSearch()
            return
        }
        let sessionID = "ios-nostr-discover-\(UUID().uuidString)"
        searchError = nil
        let outcome = store.dispatchNostrDiscoveryIntent(
            input: trimmed,
            sessionID: sessionID
        )
        switch outcome {
        case .dispatched(.textQuery):
            activeSearchSessionID = sessionID
        case .dispatched:
            activeSearchSessionID = nil
            searchError = "That input was handled as a direct Nostr reference."
        case .rejection(.secretLike):
            activeSearchSessionID = nil
            searchError = "Private-key-like input was rejected."
        case .rejection:
            activeSearchSessionID = nil
            searchError = "Nostr search could not classify that input."
        case nil:
            activeSearchSessionID = nil
            searchError = "Nostr search is unavailable."
        }
    }

    private func isAlreadySubscribed(_ show: NostrShowSummary) -> Bool {
        store.rustIsAlreadySubscribed(
            feedURL: show.feedUrl,
            ownerPubkey: show.authorPubkey
        )
    }

    private func subscribe(to show: NostrShowSummary) async {
        guard subscribingID == nil else { return }
        subscribingID = show.id
        rowErrors.removeValue(forKey: show.id)
        defer { subscribingID = nil }

        do {
            let podcast: Podcast
            if let feed = show.feedUrl, !feed.isEmpty {
                // RSS-backed NIP-F4 show: subscribe via feed URL.
                podcast = try await store.kernelSubscribe(feedURL: feed)
            } else {
                // Feedless NIP-F4 show: subscribe via author pubkey.
                // Rust opens a kind:54 relay subscription and creates a
                // feedless show row; episodes arrive asynchronously.
                podcast = try await store.kernelSubscribeNostr(
                    authorPubkeyHex: show.authorPubkey,
                    showTitle: show.title.isEmpty ? nil : show.title
                )
            }
            Haptics.success()
            onAdded(podcast)
        } catch {
            rowErrors[show.id] = error.localizedDescription
        }
    }

    private func subscribe(to hit: NostrSearchHit) async {
        guard subscribingID == nil else { return }
        subscribingID = hit.id
        rowErrors.removeValue(forKey: hit.id)
        defer { subscribingID = nil }

        do {
            let podcast = try await store.kernelSubscribeNostr(
                authorPubkeyHex: hit.author,
                showTitle: hit.displayName
            )
            Haptics.success()
            onAdded(podcast)
        } catch {
            rowErrors[hit.id] = error.localizedDescription
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

    /// All NIP-F4 shows are subscribable:
    /// - RSS-backed shows (with a `feedUrl`) via the RSS subscribe path.
    /// - Feedless shows (no `feedUrl`, but have a pubkey) via `subscribe_nostr`.
    private var isSubscribable: Bool {
        if let feed = show.feedUrl, !feed.isEmpty { return true }
        return !show.authorPubkey.isEmpty
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
        } else {
            // Both RSS-backed and feedless shows show plus.circle.fill;
            // isSubscribable is now always true (all NIP-F4 shows are subscribable).
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        }
    }

    private var accessibilityLabel: String {
        if isAlreadySubscribed { return "Already subscribed to \(show.title)" }
        if isSubscribing { return "Subscribing to \(show.title)" }
        let isFeedless = show.feedUrl?.isEmpty ?? true
        if isFeedless { return "Subscribe to \(show.title) on Nostr (feedless)" }
        return "Subscribe to \(show.title)"
    }
}
