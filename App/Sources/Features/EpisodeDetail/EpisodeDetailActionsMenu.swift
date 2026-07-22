import SwiftUI

// MARK: - EpisodeDetailActionsMenu

/// Trailing toolbar menu shown by `EpisodeDetailView`: download / mark-played
/// toggles. Routes every mutation through `AppStateStore` (state) plus
/// the shared core so durable intent and policy have one owner.
///
/// Stable file evidence comes from the episode; active and failed lifecycle
/// comes from the authoritative workflow job.
struct EpisodeDetailActionsMenu: View {
    let episode: Episode
    let store: AppStateStore

    /// Live playback model — the queue lives here. Pulled from the environment
    /// (rather than threaded through every call-site) to match the pattern used
    /// by every other feature view in the app (Home, Library, Search, Agent).
    @Environment(PlaybackState.self) private var playback
    @Environment(WorkflowClient.self) private var workflows

    @State private var confirmDelete: Bool = false
    @State private var showDiagnostics: Bool = false

    var body: some View {
        Menu {
            downloadButton
            queueButton
            Button {
                togglePlayed()
            } label: {
                Label(episode.played ? "Mark as unplayed" : "Mark as played",
                      systemImage: episode.played ? "circle" : "checkmark.circle.fill")
            }
            Divider()
            Button {
                showDiagnostics = true
            } label: {
                Label("Diagnostics", systemImage: "stethoscope")
            }
        } label: {
            Image(systemName: "ellipsis.circle")
                .font(.title3)
        }
        .accessibilityLabel("Episode options")
        // `.alert` rather than `.confirmationDialog` — anchored to a Menu,
        // iOS 26 promotes confirmationDialog to a popover that elides the
        // Cancel button. See `ShowDetailView` and `StorageSettingsView`
        // for the same trap. `.alert` is a centred modal and reliably
        // renders both buttons.
        .alert(
            "Remove download?",
            isPresented: $confirmDelete
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) {
                store.sharedLibrary?.removeDownload(episodeID: episode.id)
            }
        } message: {
            Text("The local file will be deleted. You can download it again later.")
        }
        .sheet(isPresented: $showDiagnostics) {
            EpisodeAuditLogView(episode: episode)
                .environment(store)
        }
    }

    // MARK: - Download menu item

    @ViewBuilder
    private var downloadButton: some View {
        if case .downloaded = episode.downloadState {
            Button(role: .destructive) {
                confirmDelete = true
            } label: {
                Label("Remove download", systemImage: "trash")
            }
        } else {
            switch downloadJob?.state {
            case .pending, .leased, .running, .retryScheduled:
                Button {
                    store.sharedLibrary?.cancelDownload(episodeID: episode.id)
                } label: {
                    Label("Cancel download", systemImage: "xmark.circle")
                }
            case .blocked, .failedPermanent:
                Button {
                    store.sharedLibrary?.retryDownload(episodeID: episode.id)
                } label: {
                    Label("Retry download", systemImage: "arrow.clockwise")
                }
            default:
            Button {
                store.sharedLibrary?.requestDownload(episodeID: episode.id)
            } label: {
                Label("Download", systemImage: "arrow.down.circle")
            }
            }
        }
    }

    private var downloadJob: WorkflowJobProjection? {
        workflows.latest(kind: .download, subjectID: episode.id)
    }

    // MARK: - Queue menu item

    /// "Add to queue" / "Remove from queue" toggle. The label flips so the
    /// affordance is reversible without leaving the menu — Pocket Casts and
    /// Overcast both follow this pattern. We deliberately don't hide the row
    /// when the episode is currently playing (where `enqueue` is a no-op):
    /// the user may have arrived via a direct deep link and is now reading
    /// the detail; surfacing the inert affordance would be confusing.
    @ViewBuilder
    private var queueButton: some View {
        if playback.isQueued(episode.id) {
            Button {
                Haptics.light()
                playback.removeFromQueue(episode.id)
            } label: {
                Label("Remove from queue", systemImage: "text.badge.minus")
            }
        } else {
            Button {
                Haptics.success()
                playback.enqueue(episode.id)
            } label: {
                Label("Add to queue", systemImage: "text.badge.plus")
            }
        }
    }

    private func togglePlayed() {
        if episode.played {
            store.markEpisodeUnplayed(episode.id)
        } else {
            store.markEpisodePlayed(episode.id)
        }
    }
}
