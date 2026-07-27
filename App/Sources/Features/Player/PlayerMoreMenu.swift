import SwiftUI
import UIKit

// MARK: - PlayerMoreMenu

/// Top-bar "More" pull-down for the full-height `PlayerView`.
///
/// A compact popover keeps all secondary Player actions behind one control.
/// Audio Output uses a real `AVRoutePickerView` as its row-sized hit target;
/// Apple exposes no supported API for presenting that picker from a synthetic
/// menu action.
///
/// Navigation items (Go to episode / Go to show) post a notification that
/// `RootView` observes; the handler flips `showFullPlayer = false` and the
/// target sheet's binding in the same render tick. We used to dismiss the
/// player and then async-open a `podcastr://` URL, but that raced the
/// sheet-dismissal animation — by the time `onOpenURL` resolved and toggled
/// the destination sheet, the player sheet was still mid-dismiss and SwiftUI
/// crashed trying to present a sheet over a dismissing one. The atomic
/// notification path mirrors `PlayerClipSourceChip`'s working pattern.
struct PlayerMoreMenu: View {

    let episode: Episode
    let podcast: Podcast?
    let speedLabel: String
    let onShare: () -> Void
    let onMarkPlayed: () -> Void
    let onMarkUnplayed: () -> Void
    let onShowSleepTimer: () -> Void
    let onShowSpeed: () -> Void

    /// Drives the brief "Copied!" label swap on the Copy item. Resets after
    /// `Self.copyAckDuration` so the next pull-down shows the canonical label.
    /// Menu rows can't host transient toasts, so the label flip is the most
    /// honest in-line acknowledgement we can give.
    @State private var didCopyLink: Bool = false
    @State private var isPresented: Bool = false

    /// How long the "Copied!" affordance stays visible after a copy.
    private static let copyAckDuration: Duration = .milliseconds(1_400)

    var body: some View {
        Button {
            Haptics.selection()
            isPresented.toggle()
        } label: {
            Image(systemName: "ellipsis")
                .font(.body.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .buttonStyle(.pressable)
        .accessibilityLabel("More options")
        .popover(
            isPresented: $isPresented,
            attachmentAnchor: .rect(.bounds),
            arrowEdge: .top
        ) {
            menuContent
                .presentationCompactAdaptation(.popover)
        }
    }

    private var menuContent: some View {
        VStack(spacing: 0) {
            menuButton("Share Episode", systemImage: "square.and.arrow.up") {
                Haptics.selection()
                onShare()
            }

            audioOutputRow

            Divider()

            menuButton("Speed: \(speedLabel)", systemImage: "speedometer") {
                Haptics.selection()
                onShowSpeed()
            }

            menuButton("Sleep Timer", systemImage: "moon.fill") {
                Haptics.selection()
                onShowSleepTimer()
            }

            Divider()

            menuButton(
                episode.played ? "Mark as unplayed" : "Mark as played",
                systemImage: episode.played ? "circle" : "checkmark.circle.fill"
            ) {
                Haptics.selection()
                if episode.played {
                    onMarkUnplayed()
                } else {
                    onMarkPlayed()
                }
            }

            menuButton("Go to episode", systemImage: "doc.text") {
                Haptics.selection()
                openEpisode()
            }

            if let podcast {
                menuButton("Go to show", systemImage: "rectangle.stack") {
                    Haptics.selection()
                    openShow(podcast)
                }
            }

            Divider()

            menuButton(
                didCopyLink ? "Copied!" : "Copy episode link",
                systemImage: didCopyLink ? "checkmark" : "link"
            ) {
                Haptics.success()
                UIPasteboard.general.string = episodeDeepLink
                acknowledgeCopy()
            }

            if let feedURL = podcast?.feedURL {
                menuButton(
                    "Open RSS feed",
                    systemImage: "antenna.radiowaves.left.and.right"
                ) {
                    Haptics.light()
                    UIApplication.shared.open(feedURL)
                }
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
        .frame(width: 280)
    }

    private var audioOutputRow: some View {
        ZStack {
            menuLabel("Audio Output", systemImage: "airplayaudio")
            RoutePickerView(
                activeTintColor: .clear,
                tintColor: .clear,
                accessibilityName: "Audio Output"
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .opacity(0.02)
        }
        .frame(height: 44)
    }

    private func menuButton(
        _ title: String,
        systemImage: String,
        action: @escaping () -> Void
    ) -> some View {
        Button {
            dismissThen(action)
        } label: {
            menuLabel(title, systemImage: systemImage)
        }
        .buttonStyle(.plain)
    }

    private func menuLabel(_ title: String, systemImage: String) -> some View {
        Label(title, systemImage: systemImage)
            .font(.body)
            .foregroundStyle(.primary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, AppTheme.Spacing.md)
            .frame(height: 44)
            .contentShape(Rectangle())
    }

    private func dismissThen(_ action: @escaping () -> Void) {
        isPresented = false
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(140))
            action()
        }
    }

    // MARK: - Deep-link helpers

    /// `podcastr://e/<guid>` — the lane-spec literal format. Different from
    /// the in-app `podcastr://episode/<uuid>` route the deep-link handler
    /// recognises today, but matches what the spec asks the share/copy paths
    /// to surface for forward compat with publisher-side link unfurling.
    private var episodeDeepLink: String {
        DeepLinkHandler.episodeGUIDDeepLink(guid: episode.guid)
            ?? episode.enclosureURL.absoluteString
    }

    /// Ask `RootView` to swap the player sheet for the episode-detail sheet.
    /// Both bindings flip in the same render tick on the receiver side, so
    /// SwiftUI handles the dismiss+present as a single transition.
    private func openEpisode() {
        NotificationCenter.default.post(
            name: .openEpisodeDetailRequested,
            object: nil,
            userInfo: ["episodeID": episode.id.uuidString]
        )
    }

    private func openShow(_ podcast: Podcast) {
        NotificationCenter.default.post(
            name: .openSubscriptionDetailRequested,
            object: nil,
            userInfo: ["subscriptionID": podcast.id.uuidString]
        )
    }

    /// Flip the Copy item's label/icon to the success affordance, then auto-reset
    /// so the menu reads canonically the next time it's pulled down. Detached
    /// task because the menu often dismisses on selection — we still want the
    /// reset to fire so a *re-open* before the timer expires doesn't see a
    /// stuck "Copied!" label.
    private func acknowledgeCopy() {
        didCopyLink = true
        Task { @MainActor in
            try? await Task.sleep(for: Self.copyAckDuration)
            didCopyLink = false
        }
    }
}
