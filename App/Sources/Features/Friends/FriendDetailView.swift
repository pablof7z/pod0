import SwiftUI

struct FriendDetailView: View {

    @Environment(AppStateStore.self) private var store
    let friend: Friend
    @State private var showRenameAlert = false
    @State private var newName = ""
    @State private var showCopiedFeedback = false
    @State private var showAddNote = false
    @State private var editingNote: Note? = nil
    @State private var showRemoveConfirm = false
    @Environment(\.dismiss) private var dismiss
    @Namespace private var glassNS

    private var currentFriend: Friend {
        store.friend(id: friend.id) ?? friend
    }

    /// Notes attached directly to this friend (Anchor.friend).
    private var friendNotes: [Note] {
        store.activeNotes
            .filter { note in
                guard let target = note.target else { return false }
                if case .friend(let id) = target, id == friend.id { return true }
                return false
            }
            .sorted { $0.createdAt > $1.createdAt }
    }

    private var addedDateString: String {
        "Friends since " + currentFriend.addedAt.formatted(.dateTime.month(.wide).year())
    }

    var body: some View {
        List {
            Section {
                profileHeader
                    .listRowBackground(Color.clear)
                    .listRowInsets(AppTheme.Layout.cardRowInsetsSM)
            }

            Section {
                if friendNotes.isEmpty {
                    notesEmptyCTA
                } else {
                    ForEach(friendNotes) { note in
                        NoteListRow(
                            note: note,
                            onEdit: { editingNote = note },
                            onDelete: { store.deleteNote(note.id); Haptics.delete() }
                        )
                    }
                }
            } header: {
                NotesSectionHeader(title: "Notes", count: friendNotes.count, onAdd: { showAddNote = true })
            }

            FriendConversationTranscriptView(friendPubkey: currentFriend.identifier)

            Section {
                Button("Rename") {
                    newName = currentFriend.displayName
                    showRenameAlert = true
                }
            }

            Section {
                Button("Remove Friend", role: .destructive) {
                    showRemoveConfirm = true
                }
            }
        }
        .navigationTitle(currentFriend.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showAddNote = true
                } label: {
                    Label("Add note", systemImage: "square.and.pencil")
                }
            }
        }
        // Two-step confirm before removal — matches the Unsubscribe
        // dialog pattern in `ShowDetailView`. The previous one-tap
        // `Button(role: .destructive)` was a real footgun: tapping the
        // row immediately wiped the friend and the Nostr-allowed
        // pubkey, with no undo, while leaving any notes orphaned.
        .alert(
            "Remove \(currentFriend.displayName) from friends?",
            isPresented: $showRemoveConfirm
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) {
                store.removeFriend(friend.id)
                Haptics.delete()
                dismiss()
            }
        } message: {
            Text("They'll disappear from your friends list and stop receiving inbound replies. Any notes you've written about them stay.")
        }
        .alert("Rename Friend", isPresented: $showRenameAlert) {
            TextField("Display Name", text: $newName)
            Button("Save") {
                let t = newName.trimmed
                guard !t.isEmpty else { return }
                store.updateFriendDisplayName(friend.id, newName: t)
                Haptics.success()
            }
            Button("Cancel", role: .cancel) {}
        }
        .sheet(isPresented: $showAddNote) {
            EditTextSheet(title: "Note about \(currentFriend.displayName)", initialText: "") { text in
                store.addNote(text: text, kind: .free, target: .friend(id: friend.id))
                Haptics.success()
            }
        }
        .sheet(item: $editingNote) { note in
            EditTextSheet(title: "Edit Note", initialText: note.text) { newText in
                var updated = note
                updated.text = newText
                store.updateNote(updated)
            }
        }
    }

    // MARK: - Notes section helpers

    private var notesEmptyCTA: some View {
        Button {
            showAddNote = true
        } label: {
            Label("Add a note about \(currentFriend.displayName)…", systemImage: "plus.circle")
                .font(AppTheme.Typography.callout)
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
    }

    // MARK: - Profile header

    @ViewBuilder
    private var profileHeader: some View {
        HStack(spacing: AppTheme.Spacing.lg) {
            FriendAvatar(friend: currentFriend, size: 68)

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(currentFriend.displayName)
                    .font(AppTheme.Typography.title)

                Button {
                    copyToClipboard(currentFriend.identifier, isCopied: $showCopiedFeedback)
                } label: {
                    HStack(spacing: AppTheme.Spacing.xs) {
                        if showCopiedFeedback {
                            Label("Copied!", systemImage: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        } else {
                            Label(currentFriend.identifier, systemImage: "doc.on.doc")
                                .foregroundStyle(.secondary)
                        }
                    }
                    .font(AppTheme.Typography.mono)
                    .truncatedMiddle()
                    .contentTransition(.identity)
                }
                .buttonStyle(.plain)
                .animation(AppTheme.Animation.springFast, value: showCopiedFeedback)
                .accessibilityLabel(showCopiedFeedback ? "Identifier copied" : "Copy identifier")
                .accessibilityHint("Copies the friend's identifier to the clipboard")

                Text(addedDateString)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.tertiary)

                if let about = currentFriend.about {
                    Text(about)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(cornerRadius: AppTheme.Corner.xl)
        .glassEffectID("profile-\(friend.id)", in: glassNS)
    }
}
