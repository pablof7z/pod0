import SwiftUI

// MARK: - OwnedPodcastsView
//
// NIP-F4 owned podcast publishing surface (features #27/#28). Lists every
// subscribed podcast and, per row, lets the user:
//
//   1. "Create Owned Identity"   — dispatches `podcast.publish.create_owned_podcast`
//      so the kernel mints a per-podcast Nostr keypair.
//   2. "Publish Show"            — dispatches `podcast.publish.publish_show` (kind:10154).
//   3. "Publish Author Claim"    — dispatches `podcast.publish.publish_author_claim`
//      (kind:10064) under the current agent's pubkey, enumerating every
//      owned podcast's per-podcast pubkey under "p" tags.
//   4. "Remove Identity"         — dispatches `podcast.publish.remove_owned_podcast`.
//
// The relay broadcast itself returns `relay_pending` from the kernel for now
// (the broader NMP Nostr signing pipeline is still being wired through to
// per-podcast keys). The "last published" stamp is therefore a "last built"
// stamp, but the wire shape is the same once relay publishing lands.

struct OwnedPodcastsView: View {

    @Environment(KernelModel.self) private var model

    var body: some View {
        List {
            ownerActionsSection
            podcastsSection
            if !ownedRows.isEmpty {
                ownedSection
            }
        }
        .navigationTitle("Your Podcasts")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Sections

    private var ownerActionsSection: some View {
        Section {
            Button {
                publishAuthorClaim()
            } label: {
                Label("Publish Author Claim", systemImage: "person.badge.shield.checkmark")
            }
            .disabled(ownedRows.isEmpty || agentPubkeyHex.isEmpty)
        } header: {
            Text("Agent")
        } footer: {
            if agentPubkeyHex.isEmpty {
                Text("Sign in to publish an author claim.")
            } else if ownedRows.isEmpty {
                Text("Create at least one owned podcast identity before publishing the claim.")
            } else {
                Text("Publishes a kind:10064 event under your agent key declaring ownership of every podcast keypair listed below.")
            }
        }
    }

    private var podcastsSection: some View {
        Section("Subscribed Podcasts") {
            if model.library.isEmpty {
                Text("No subscribed podcasts yet.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(model.library) { podcast in
                    podcastRow(podcast)
                }
            }
        }
    }

    private var ownedSection: some View {
        Section("Owned Identities") {
            ForEach(ownedRows) { info in
                OwnedPodcastRow(
                    info: info,
                    podcastTitle: titleFor(podcastId: info.podcastId),
                    onPublishShow: { publishShow(podcastId: info.podcastId) },
                    onRemove: { removeOwned(podcastId: info.podcastId) }
                )
            }
        }
    }

    // MARK: - Row factories

    @ViewBuilder
    private func podcastRow(_ podcast: PodcastSummary) -> some View {
        let isOwned = ownedSet.contains(podcast.id)
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(podcast.title)
                    .font(.body)
                if isOwned {
                    Text("Identity created")
                        .font(.caption)
                        .foregroundStyle(.green)
                }
            }
            Spacer()
            if !isOwned {
                Button("Create Owned Identity") {
                    createOwned(podcastId: podcast.id)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            }
        }
    }

    // MARK: - Derived state

    private var ownedRows: [OwnedPodcastInfo] {
        model.podcastSnapshot?.ownedPodcasts ?? []
    }

    private var ownedSet: Set<String> {
        Set(ownedRows.map { $0.podcastId })
    }

    private var agentPubkeyHex: String {
        // Compat shim: `Bech32.encode` produces `npub1<hex>`; reverse that
        // to get the hex bytes the kernel-side action module expects. When
        // real bech32 lands the prefix-strip becomes a proper bech32 decode.
        guard let npub = model.podcastSnapshot?.activeAccount?.npub else { return "" }
        let prefix = "npub1"
        if npub.hasPrefix(prefix) {
            return String(npub.dropFirst(prefix.count))
        }
        return npub
    }

    private func titleFor(podcastId: String) -> String {
        model.library.first { $0.id == podcastId }?.title ?? podcastId
    }

    // MARK: - Dispatch helpers

    private func createOwned(podcastId: String) {
        Haptics.medium()
        model.dispatch(namespace: "podcast.publish", body: [
            "op": "create_owned_podcast",
            "podcast_id": podcastId,
        ])
    }

    private func publishShow(podcastId: String) {
        Haptics.medium()
        model.dispatch(namespace: "podcast.publish", body: [
            "op": "publish_show",
            "podcast_id": podcastId,
        ])
    }

    private func publishAuthorClaim() {
        Haptics.medium()
        model.dispatch(namespace: "podcast.publish", body: [
            "op": "publish_author_claim",
            "agent_pubkey_hex": agentPubkeyHex,
        ])
    }

    private func removeOwned(podcastId: String) {
        Haptics.warning()
        model.dispatch(namespace: "podcast.publish", body: [
            "op": "remove_owned_podcast",
            "podcast_id": podcastId,
        ])
    }
}

// MARK: - Row

private struct OwnedPodcastRow: View {

    let info: OwnedPodcastInfo
    let podcastTitle: String
    let onPublishShow: () -> Void
    let onRemove: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(podcastTitle)
                .font(.headline)
            HStack(spacing: 4) {
                Text("Key:")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(truncatedPubkey)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.primary)
            }
            if let lastPublished = lastPublishedText {
                Text("Last published: \(lastPublished)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else {
                Text("Not yet published")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            HStack(spacing: 8) {
                Button("Publish Show", action: onPublishShow)
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                Spacer()
                Button(role: .destructive, action: onRemove) {
                    Text("Remove")
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
        .padding(.vertical, 4)
    }

    private var truncatedPubkey: String {
        let pk = info.podcastPubkeyHex
        guard pk.count > 14 else { return pk }
        return "\(pk.prefix(8))…\(pk.suffix(6))"
    }

    private var lastPublishedText: String? {
        guard let ts = info.lastPublishedAt else { return nil }
        return relativeDate(from: Date(timeIntervalSince1970: TimeInterval(ts)))
    }
}
