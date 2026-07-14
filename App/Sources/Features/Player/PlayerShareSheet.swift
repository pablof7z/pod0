import SwiftUI
import UIKit

// MARK: - PlayerShareSheet

/// Share sheet presented from the player's Share chip.
///
/// Targets, in order:
///   - **Copy episode link** — `podcastr://e/<guid>` deep link.
///   - **Copy link with timestamp** — same, with `?t=<seconds>` appended so
///     a recipient lands at the current playhead.
///   - **Share via system** — SwiftUI `ShareLink` over the deep link.
///   - **Share quote** — presents `QuoteShareView` for the segment at the
///     current time. Gated on `episode.transcriptState == .ready` (which
///     means hidden in this lane until lane 5 / transcript ingestion lands —
///     `PlayerTranscriptScrollView` is also a placeholder in this build).
struct PlayerShareSheet: View {

    @Environment(\.dismiss) private var dismiss
    @Environment(AppStateStore.self) private var store
    @Bindable var state: PlaybackState
    let episode: Episode
    let showName: String

    /// Threshold below which a "share at current time" link would be
    /// indistinguishable from a fresh-start share. Picked at 5s so a brief
    /// pre-roll skim doesn't spuriously enable the row.
    private static let timestampedShareMinSeconds: TimeInterval = 5

    /// Resolved transcript segment at the current playhead, surfaced via the
    /// quote-share sheet. Set when the user taps "Share quote"; reset when the
    /// sheet dismisses. Optional `Segment` is `Identifiable`, which `sheet(item:)`
    /// requires.
    @State private var quotingSegment: Segment?

    /// True while the LLM is resolving boundaries for "Share quote". The row
    /// swaps its glyph for a spinner so the user sees the latency is purposeful
    /// instead of dead-air.
    @State private var quoteResolving: Bool = false

    var body: some View {
        NavigationStack {
            VStack(spacing: AppTheme.Spacing.md) {
                copyLinkButton
                if hasMeaningfulPlayhead {
                    copyTimestampedLinkButton
                }
                systemShareButton
                if hasReadyTranscript {
                    shareQuoteButton
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.top, AppTheme.Spacing.lg)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .navigationTitle("Share")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(item: $quotingSegment) { segment in
                quoteSheet(for: segment)
            }
        }
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
    }

    // MARK: - Targets

    private var copyLinkButton: some View {
        shareRow(label: "Copy episode link", systemImage: "link") {
            Haptics.light()
            UIPasteboard.general.string = episodeDeepLink
        }
    }

    private var copyTimestampedLinkButton: some View {
        shareRow(label: "Copy link at current time", systemImage: "clock") {
            Haptics.light()
            UIPasteboard.general.string = timestampedDeepLink
        }
    }

    @ViewBuilder
    private var systemShareButton: some View {
        if let url = URL(string: episodeDeepLink) {
            // `subject` only surfaces in some destinations (Mail's subject
            // line). The share-sheet preview itself defaults to the URL's
            // own metadata, which for a `podcastr://e/<guid>` deep link
            // is just the bare scheme + path — no episode context. The
            // explicit `SharePreview` makes the destination header read
            // "<Show>: <Episode>" so the recipient sees what they're
            // about to receive. Mirrors the per-row context-menu share.
            ShareLink(
                item: url,
                subject: Text(episode.title),
                preview: SharePreview(sharePreviewTitle, image: Image(systemName: "headphones"))
            ) {
                shareRowLabel(label: "Share via…", systemImage: "square.and.arrow.up")
            }
            .buttonStyle(.plain)
        }
    }

    private var sharePreviewTitle: String {
        showName.isEmpty ? episode.title : "\(showName): \(episode.title)"
    }

    private var shareQuoteButton: some View {
        Button(action: { presentQuoteAtPlayhead() }) {
            HStack(spacing: AppTheme.Spacing.md) {
                Group {
                    if quoteResolving {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Image(systemName: "text.quote")
                            .font(.body.weight(.semibold))
                    }
                }
                .frame(width: 22, alignment: .center)
                Text(quoteResolving ? "Finding a clean quote…" : "Share quote")
                    .font(AppTheme.Typography.subheadline)
                Spacer(minLength: 0)
            }
            .foregroundStyle(.primary)
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, 14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(.regular.interactive(), in: .capsule)
            .accessibilityLabel(quoteResolving ? "Finding a clean quote" : "Share quote")
        }
        .buttonStyle(.pressable)
        .disabled(quoteResolving)
    }

    /// Load the persisted transcript for this episode, ask the LLM to pick
    /// semantic boundaries around the playhead, and present `QuoteShareView`
    /// for the resulting span. On any failure (no key, network blip, malformed
    /// response, no transcript) we fall back to today's single-segment behavior
    /// so the share affordance still works — same defensive path the previous
    /// implementation took, just preceded by an LLM round-trip when possible.
    private func presentQuoteAtPlayhead() {
        guard let transcript = EpisodeDetailView.readyTranscript(for: episode) else {
            Haptics.error()
            return
        }
        Haptics.light()
        let modelID = store.state.settings.wikiModel
        let modelReference = LLMModelReference(storedID: modelID)
        // Surface the one-time hint when we'd otherwise silently degrade.
        // Falls through to the mechanical fallback below regardless.
        if !LLMProviderCredentialResolver.hasAPIKey(for: modelReference.provider) {
            AutoSnipController.shared.noLLMKeyHintPending = true
            quotingSegment = transcript.segment(at: state.currentTime)
            return
        }
        quoteResolving = true
        let playhead = state.currentTime
        Task { @MainActor in
            defer { quoteResolving = false }
            let resolved = await ClipBoundaryResolver.shared.resolveBoundaries(
                transcript: transcript,
                playheadSeconds: playhead,
                intent: .quote,
                modelID: modelID
            )
            if let resolved {
                quotingSegment = Segment(
                    start: resolved.startSeconds,
                    end: resolved.endSeconds,
                    speakerID: resolved.speakerID,
                    text: resolved.quotedText
                )
            } else {
                // Mechanical fallback so a failed LLM call still lets the
                // user share something. Same shape as the pre-LLM behavior.
                quotingSegment = transcript.segment(at: playhead)
            }
        }
    }

    @ViewBuilder
    private func quoteSheet(for segment: Segment) -> some View {
        let transcript = EpisodeDetailView.readyTranscript(for: episode)
        QuoteShareView(
            episode: episode,
            showName: showName,
            showImageURL: episode.imageURL,
            segment: segment,
            speaker: transcript?.speaker(for: segment.speakerID),
            deepLink: quoteDeepLink(for: segment)
        )
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
    }

    private func quoteDeepLink(for segment: Segment) -> String {
        DeepLinkHandler.episodeGUIDDeepLink(guid: episode.guid, startTime: segment.start)
            ?? episode.enclosureURL.absoluteString
    }

    // MARK: - Row plumbing

    private func shareRow(label: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            shareRowLabel(label: label, systemImage: systemImage)
        }
        .buttonStyle(.pressable)
    }

    private func shareRowLabel(label: String, systemImage: String) -> some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Image(systemName: systemImage)
                .font(.body.weight(.semibold))
                .frame(width: 22, alignment: .center)
            Text(label)
                .font(AppTheme.Typography.subheadline)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.primary)
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassEffect(.regular.interactive(), in: .capsule)
        .accessibilityLabel(label)
    }

    // MARK: - Deep-link helpers

    /// Spec literal: `podcastr://e/<guid>`. Distinct from the in-app
    /// `podcastr://episode/<uuid>` route the deep-link handler currently
    /// recognises — kept this way for forward compat with publisher-side
    /// link unfurling once a `e/` route lands.
    private var episodeDeepLink: String {
        DeepLinkHandler.episodeGUIDDeepLink(guid: episode.guid)
            ?? episode.enclosureURL.absoluteString
    }

    private var timestampedDeepLink: String {
        DeepLinkHandler.episodeGUIDDeepLink(guid: episode.guid, startTime: state.currentTime)
            ?? episodeDeepLink
    }

    private var hasReadyTranscript: Bool {
        if case .ready = episode.transcriptState { return true }
        return false
    }

    /// True when the playhead is far enough into the episode that a "share at
    /// current time" link carries meaningful information beyond a fresh-start
    /// share. Pulled out as a helper (with an internal-visible static twin
    /// below) so the predicate can be unit-tested without standing up a
    /// SwiftUI view hierarchy.
    var hasMeaningfulPlayhead: Bool {
        Self.isMeaningfulPlayhead(state.currentTime)
    }

    /// Pure predicate for the timestamp-share gate. Exposed for tests.
    static func isMeaningfulPlayhead(_ currentTime: TimeInterval) -> Bool {
        currentTime > timestampedShareMinSeconds
    }
}
