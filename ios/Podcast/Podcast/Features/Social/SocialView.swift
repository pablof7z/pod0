import SwiftUI

// MARK: - SocialView
//
// Top-level "Social" tab. Reads the NIP-02 (kind:3) follow list from
// `model.podcastSnapshot?.social?.following` and renders an avatar grid.
//
// State machine:
//   * `snapshot.social == nil`  → pre-fetch / loading (the kernel projection
//     hasn't surfaced the contact list yet, so we show a progress view).
//   * `snapshot.social?.following.isEmpty == true` → fetched but empty
//     (no follows). We render an actionable empty state.
//   * otherwise → grid of `ContactRow` tiles.
//
// On appear we dispatch `podcast.fetch_contacts`. For this PR that returns
// `{"ok":true,"status":"nostr_pending"}` — the loading state holds until the
// NMP substrate contact store is wired into the projection layer
// (`pr-social-graph-nmp-store-wiring` in `docs/BACKLOG.md`).

struct SocialView: View {
    @Environment(KernelModel.self) private var model

    private let columns = [GridItem(.adaptive(minimum: 96), spacing: AppTheme.Spacing.md)]

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("Following")
                .onAppear { fetchContacts() }
                .refreshable { fetchContacts() }
        }
    }

    // MARK: Content branching

    @ViewBuilder
    private var content: some View {
        if let social = model.podcastSnapshot?.social {
            if social.following.isEmpty {
                emptyState
            } else {
                followingGrid(social.following)
            }
        } else {
            loadingState
        }
    }

    // MARK: Branches

    private var loadingState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            ProgressView()
            Text("Loading your Nostr contacts…")
                .font(AppTheme.Typography.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Loading your Nostr contacts")
    }

    private var emptyState: some View {
        ContentUnavailableView(
            "No Contacts Yet",
            systemImage: "person.2",
            description: Text("You aren't following anyone on Nostr yet. Once you follow people, they'll appear here.")
        )
    }

    private func followingGrid(_ contacts: [ContactSummary]) -> some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: AppTheme.Spacing.lg) {
                ForEach(contacts) { contact in
                    ContactRow(contact: contact)
                }
            }
            .padding(AppTheme.Spacing.md)
        }
    }

    // MARK: Actions

    private func fetchContacts() {
        model.dispatch(namespace: "podcast", body: ["op": "fetch_contacts"])
    }
}
