import SwiftUI
import UIKit

// MARK: - PlayerMoreMenu

/// Top-bar "More" actions for the full-screen `PlayerView`.
///
/// The trigger keeps the same compact glass control as the rest of the player
/// chrome, while the actions render in an app-owned sheet. This avoids the
/// UIKit pull-down idleness stalls we saw when changing playback speed through
/// a system `Menu` on the iOS simulator.
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

    @Bindable var state: PlaybackState
    let episode: Episode
    let podcast: Podcast?
    let onMarkPlayed: () -> Void
    let onMarkUnplayed: () -> Void
    let onShowSleepTimer: () -> Void
    let onShowQueue: () -> Void

    /// Drives the brief "Copied!" label swap on the Copy item. Resets after
    /// `Self.copyAckDuration` so the next pull-down shows the canonical label.
    /// Menu items can't host transient toasts, so the label flip is the most
    /// honest in-line acknowledgement we can give.
    @State private var didCopyLink: Bool = false
    @State private var showOptionsSheet: Bool = false
    @State private var showingSpeedChoices: Bool = false

    /// How long the "Copied!" affordance stays visible after a copy.
    private static let copyAckDuration: Duration = .milliseconds(1_400)

    var body: some View {
        Button {
            Haptics.selection()
            showingSpeedChoices = false
            showOptionsSheet = true
        } label: {
            Image(systemName: "ellipsis")
                .font(.body.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(Circle())
                .glassEffect(.regular.interactive(), in: .circle)
        }
        .buttonStyle(.pressable)
        .accessibilityLabel("More options")
        .sheet(isPresented: $showOptionsSheet) {
            NavigationStack {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        if showingSpeedChoices {
                            speedChoices
                        } else {
                            optionsList
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.bottom, AppTheme.Spacing.lg)
                }
                .navigationTitle(showingSpeedChoices ? "Playback Speed" : "More Options")
                .toolbarTitleDisplayMode(.inline)
                .toolbar {
                    if showingSpeedChoices {
                        ToolbarItem(placement: .topBarLeading) {
                            Button("Back") {
                                Haptics.selection()
                                showingSpeedChoices = false
                            }
                        }
                    }
                }
            }
            .presentationDetents([.medium, .large])
            .presentationDragIndicator(.visible)
        }
    }

    @ViewBuilder
    private var optionsList: some View {
        optionButton("Up Next", systemImage: "list.number") {
            dismissOptionsThen(onShowQueue)
        }
        .accessibilityIdentifier("player-queue-chip")

        optionButton("Speed: \(state.rate.label)", systemImage: "speedometer") {
            Haptics.selection()
            showingSpeedChoices = true
        }

        optionButton("Sleep Timer", systemImage: "moon.fill") {
            dismissOptionsThen(onShowSleepTimer)
        }

        Divider()
            .padding(.vertical, AppTheme.Spacing.xs)

        optionButton(
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

        optionButton("Go to episode", systemImage: "doc.text") {
            Haptics.selection()
            showOptionsSheet = false
            openEpisode()
        }

        if let podcast {
            optionButton("Go to show", systemImage: "rectangle.stack") {
                Haptics.selection()
                showOptionsSheet = false
                openShow(podcast)
            }
        }

        Divider()
            .padding(.vertical, AppTheme.Spacing.xs)

        optionButton(didCopyLink ? "Copied!" : "Copy episode link", systemImage: didCopyLink ? "checkmark" : "link") {
            Haptics.success()
            UIPasteboard.general.string = episodeDeepLink
            acknowledgeCopy()
        }

        if let feedURL = podcast?.feedURL {
            optionButton("Open RSS feed", systemImage: "antenna.radiowaves.left.and.right") {
                Haptics.light()
                UIApplication.shared.open(feedURL)
            }
        }
    }

    private var speedChoices: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(PlaybackRate.allCases) { rate in
                Button {
                    state.setRate(rate)
                    Haptics.selection()
                    showOptionsSheet = false
                } label: {
                    HStack {
                        Text(rate.label)
                            .font(AppTheme.Typography.headline)
                            .foregroundStyle(.primary)
                        Spacer()
                        if rate == state.rate {
                            Image(systemName: "checkmark")
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundStyle(.tint)
                                .accessibilityHidden(true)
                        }
                    }
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    .padding(.vertical, AppTheme.Spacing.md)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.pressable(scale: 0.98, opacity: 0.85))
                .accessibilityAddTraits(rate == state.rate ? [.isButton, .isSelected] : .isButton)
            }
        }
    }

    private func optionButton(_ title: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .font(AppTheme.Typography.body)
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.vertical, AppTheme.Spacing.md)
                .contentShape(Rectangle())
        }
        .buttonStyle(.pressable(scale: 0.98, opacity: 0.85))
    }

    private func dismissOptionsThen(_ action: @escaping () -> Void) {
        Haptics.selection()
        showOptionsSheet = false
        Task { @MainActor in
            // Let this sheet finish dismissal before asking the parent to
            // present its queue/sleep sheet.
            try? await Task.sleep(for: .milliseconds(250))
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
