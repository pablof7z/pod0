import SwiftUI
import UIKit

struct AgentAccessControlView: View {

    private enum Layout {
        static let pickerVerticalPadding: CGFloat = AppTheme.Spacing.sm + AppTheme.Spacing.xs
    }

    private enum AccessTab: String, CaseIterable {
        case allowed = "Allowed"
        case blocked = "Blocked"
    }

    @Environment(AppStateStore.self) private var store

    @State private var selectedTab: AccessTab = .allowed
    @State private var searchText = ""
    @State private var showAddSheet = false

    var body: some View {
        VStack(spacing: 0) {
            LiquidGlassSegmentedPicker(
                "Tab",
                selection: $selectedTab,
                segments: AccessTab.allCases.map { ($0, tabLabel($0)) }
            )
            .padding(.horizontal)
            .padding(.vertical, Layout.pickerVerticalPadding)

            List {
                switch selectedTab {
                case .allowed:  allowedContent
                case .blocked:  blockedContent
                }
            }
            .animation(AppTheme.Animation.springFast, value: selectedTab)
        }
        .navigationTitle("Access Control")
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: searchPrompt)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showAddSheet = true } label: {
                    Label("Add", systemImage: "plus")
                }
            }
        }
        .sheet(isPresented: $showAddSheet) { addSheet }
    }

    // MARK: - Tab labels

    private func tabLabel(_ tab: AccessTab) -> String {
        switch tab {
        case .allowed:
            let count = approvedPubkeys.count
            return count > 0 ? "Allowed (\(count))" : "Allowed"
        case .blocked:
            let count = blockedPubkeys.count
            return count > 0 ? "Blocked (\(count))" : "Blocked"
        }
    }

    private var searchPrompt: String {
        switch selectedTab {
        case .allowed:  return "Search allowed peers"
        case .blocked:  return "Search blocked peers"
        }
    }

    // MARK: - Allowed

    private var approvedPubkeys: [String] {
        store.kernel?.podcastSnapshot?.social?.approvedPubkeys.sorted() ?? []
    }

    private var blockedPubkeys: [String] {
        store.kernel?.podcastSnapshot?.social?.blockedPubkeys.sorted() ?? []
    }

    private var filteredAllowed: [String] {
        let q = searchText.trimmed
        return q.isEmpty ? approvedPubkeys : approvedPubkeys.filter { $0.localizedCaseInsensitiveContains(q) }
    }

    @ViewBuilder
    private var allowedContent: some View {
        if filteredAllowed.isEmpty {
            if searchText.isEmpty {
                ContentUnavailableView {
                    Label("No allowed peers", systemImage: "checkmark.shield")
                } description: {
                    Text("Peers you explicitly approve will appear here. Followed contacts are trusted automatically.")
                } actions: {
                    Button("Add a peer") { showAddSheet = true }
                        .buttonStyle(.glassProminent)
                }
                .listRowBackground(Color.clear)
            }
        } else {
            Section {
                ForEach(filteredAllowed, id: \.self) { key in
                    AllowedRow(key: key)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                store.removeFromNostrAllowlist(key)
                                Haptics.selection()
                            } label: {
                                Label("Remove", systemImage: "trash")
                            }
                        }
                }
            }
        }
    }

    // MARK: - Blocked

    private var filteredBlocked: [String] {
        let q = searchText.trimmed
        return q.isEmpty ? blockedPubkeys : blockedPubkeys.filter { $0.localizedCaseInsensitiveContains(q) }
    }

    @ViewBuilder
    private var blockedContent: some View {
        if filteredBlocked.isEmpty {
            ContentUnavailableView {
                Label("No blocked peers", systemImage: "nosign")
            } description: {
                Text("Peers you block can never contact your agent.")
            } actions: {
                Button("Block a peer") { showAddSheet = true }
                    .buttonStyle(.glassProminent)
            }
            .listRowBackground(Color.clear)
        } else {
            Section {
                ForEach(filteredBlocked, id: \.self) { key in
                    BlockedPeerRow(key: key)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                store.removeFromNostrBlocklist(key)
                                Haptics.selection()
                            } label: {
                                Label("Unblock", systemImage: "checkmark.circle")
                            }.tint(AppTheme.Tint.warning)
                        }
                }
            }
        }
    }

    // MARK: - Add sheet

    @ViewBuilder
    private var addSheet: some View {
        switch selectedTab {
        case .allowed:
            AllowPeerSheet { hex in store.allowNostrPubkey(hex); Haptics.success() }
        case .blocked:
            BlockPeerSheet { hex in store.blockNostrPubkey(hex); Haptics.success() }
        }
    }


    // MARK: - BlockedPeerRow

    private struct BlockedPeerRow: View {
    let key: String

    @State private var isCopied = false

    var body: some View {
        Button { copyToClipboard(key, isCopied: $isCopied) } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "nosign").foregroundStyle(AppTheme.Tint.error)
                Text(NostrNpub.shortNpub(fromHex: key))
                    .font(AppTheme.Typography.monoCallout)
                    .foregroundStyle(.primary)
                Spacer()
                if isCopied {
                    Label("Copied", systemImage: "checkmark")
                        .labelStyle(.titleAndIcon)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .transition(.opacity)
                }
            }
        }
        .buttonStyle(.plain)
        .contentShape(Rectangle())
        .accessibilityLabel(isCopied ? "Copied" : "Copy public key")
        .animation(AppTheme.Animation.easeOut, value: isCopied)
    }
    }

    // MARK: - BlockPeerSheet

    private struct BlockPeerSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var hexInput: String = ""
    let onBlock: (String) -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Hex pubkey…", text: $hexInput)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .font(AppTheme.Typography.monoCallout)
                } footer: {
                    Text("Paste a Nostr public key in hex format. The peer will be blocked from contacting your agent.")
                }
            }
            .navigationTitle("Block Peer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Block") {
                        let trimmed = hexInput.trimmed.lowercased()
                        guard !trimmed.isEmpty else { return }
                        onBlock(trimmed)
                        dismiss()
                    }
                    .fontWeight(.semibold)
                    .disabled(hexInput.isBlank)
                }
            }
        }
    }
    }
}
