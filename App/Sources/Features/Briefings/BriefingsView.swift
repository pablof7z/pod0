import SwiftUI

// MARK: - BriefingsView

/// Library shelf for past briefings + Compose entry point. Surface W5
/// (saved-briefing detail) and W1 (compose) of UX-08 are both reached from
/// this view; W2 (player) is a navigation destination.
///
/// The library is tint-segregated from regular podcast episodes per UX-08 §3
/// — a brass-amber `.glassSurface` band wraps every row so a briefing is
/// never mistaken for an episode.
struct BriefingsView: View {

    // MARK: Model

    /// View-model holding the list of saved briefings and the active compose
    /// flow. Owned by the view rather than the global `AppStateStore` so this
    /// lane stays self-contained and other lanes don't have to know about it.
    @State private var model = BriefingsViewModel()

    // MARK: Sheets / nav

    @State private var isComposing = false
    @State private var pendingPlayback: BriefingPlaybackContext?
    /// Drives the lean-back river presentation. `true` while
    /// `BriefingRiverView` is on the navigation stack.
    @State private var isPlayingRiver = false

    // MARK: Body

    var body: some View {
        ScrollView {
            VStack(spacing: AppTheme.Spacing.lg) {
                if model.isUsingEphemeralStorage {
                    ephemeralStorageBanner
                }
                if let error = model.composeError {
                    errorBanner(message: error)
                }
                presetRow
                Divider().padding(.horizontal)
                if model.briefings.isEmpty {
                    emptyState
                } else {
                    libraryList
                }
            }
            .padding(.top)
        }
        .background(briefingBackground)
        .overlay(alignment: .bottom) {
            if let progress = model.composeProgress, progress != .finished {
                composeProgressOverlay(progress: progress)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.easeInOut(duration: 0.2), value: model.composeProgress)
        .navigationTitle("Briefings")
        .toolbar { toolbar }
        .sheet(isPresented: $isComposing) {
            BriefingComposeSheet(
                onCompose: { request in
                    isComposing = false
                    Task { await model.compose(request: request) }
                }
            )
        }
        .navigationDestination(item: $pendingPlayback) { ctx in
            BriefingPlayerView(context: ctx)
        }
        .navigationDestination(isPresented: $isPlayingRiver) {
            BriefingRiverView(queue: model.briefings)
        }
        .task { await model.reload() }
    }

    // MARK: Ephemeral-storage warning banner

    private var ephemeralStorageBanner: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            Image(systemName: "externaldrive.badge.exclamationmark")
                .foregroundStyle(AppTheme.Tint.warning)
            VStack(alignment: .leading, spacing: 2) {
                Text("Briefings stored temporarily")
                    .font(.subheadline.weight(.semibold))
                Text("Briefings are stored temporarily and will be lost when the app restarts. Free up storage and reopen the app.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(4)
            }
            Spacer()
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: AppTheme.Tint.warning.opacity(0.18)
        )
        .padding(.horizontal)
    }

    // MARK: Error banner

    @ViewBuilder
    private func errorBanner(message: String) -> some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(AppTheme.Tint.warning)
            VStack(alignment: .leading, spacing: 2) {
                Text("Couldn't compose briefing")
                    .font(.subheadline.weight(.semibold))
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            Spacer()
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: AppTheme.Tint.warning.opacity(0.18)
        )
        .padding(.horizontal)
    }

    // MARK: Compose progress overlay

    @ViewBuilder
    private func composeProgressOverlay(progress: BriefingComposeProgress) -> some View {
        HStack(spacing: AppTheme.Spacing.md) {
            ProgressView()
                .progressViewStyle(.circular)
            VStack(alignment: .leading, spacing: 2) {
                Text("Composing your briefing")
                    .font(.subheadline.weight(.semibold))
                Text(progressLabel(progress))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: BriefingsView.brassAmber.opacity(0.32)
        )
        .padding(.horizontal)
        .padding(.bottom, AppTheme.Spacing.lg)
    }

    private func progressLabel(_ progress: BriefingComposeProgress) -> String {
        switch progress {
        case .selectedEpisodes(let count):
            count > 0 ? "Selected \(count) episodes" : "Searching your library…"
        case .draftedSegments(let count):
            "Drafted \(count) segment\(count == 1 ? "" : "s")"
        case .synthesizingVoice(let i, let total):
            "Synthesizing voice (\(i + 1)/\(total))"
        case .stitchingQuotes:
            "Stitching audio together"
        case .finished:
            "Done"
        }
    }

    // MARK: Toolbar

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if !model.briefings.isEmpty {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    Haptics.selection()
                    isPlayingRiver = true
                } label: {
                    Label("Lean back", systemImage: "play.square.stack.fill")
                }
                .accessibilityHint("Auto-plays every briefing in sequence")
            }
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button { isComposing = true } label: {
                Label("Compose", systemImage: "sparkles")
            }
        }
    }

    // MARK: Preset row

    private var presetRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: AppTheme.Spacing.md) {
                ForEach(BriefingStyle.allCases, id: \.self) { style in
                    Button {
                        Task {
                            await model.composeQuick(style: style)
                        }
                    } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Image(systemName: icon(for: style))
                                .font(.title2)
                            Text(style.displayLabel)
                                .font(.headline)
                            Text(blurb(for: style))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        .padding(AppTheme.Spacing.md)
                        .frame(width: 180, alignment: .leading)
                    }
                    .glassSurface(
                        cornerRadius: AppTheme.Corner.lg,
                        tint: BriefingsView.brassAmber.opacity(0.18),
                        interactive: true
                    )
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal)
        }
    }

    private func icon(for style: BriefingStyle) -> String {
        switch style {
        case .morning:            "sunrise.fill"
        case .weeklyTLDR:         "calendar.badge.clock"
        case .catchUpOnShow:      "arrow.uturn.backward.circle.fill"
        case .topicAcrossLibrary: "magnifyingglass.circle.fill"
        }
    }

    private func blurb(for style: BriefingStyle) -> String {
        switch style {
        case .morning:            "What matters today, in 8 minutes."
        case .weeklyTLDR:         "The week, condensed."
        case .catchUpOnShow:      "Reconstruct an arc you missed."
        case .topicAcrossLibrary: "Reconcile across shows."
        }
    }

    // MARK: Library list

    private var libraryList: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            ForEach(model.briefings) { briefing in
                Button {
                    pendingPlayback = BriefingPlaybackContext(script: briefing)
                } label: {
                    BriefingsLibraryRow(script: briefing)
                }
                .buttonStyle(.plain)
                .padding(.horizontal)
                .contextMenu {
                    Button(role: .destructive) {
                        model.delete(briefing)
                    } label: {
                        Label("Delete", systemImage: "trash")
                    }
                }
            }
        }
    }

    // MARK: Empty state

    private var emptyState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Image(systemName: "newspaper")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            Text("No briefings yet")
                .font(.title3.weight(.semibold))
            Text("Tap a preset above or compose a freeform briefing.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
        }
        .padding(.vertical, 60)
    }

    // MARK: Background

    private var briefingBackground: some View {
        LinearGradient(
            colors: [
                BriefingsView.brassAmber.opacity(0.08),
                BriefingsView.brassAmber.opacity(0.04),
                Color(.systemBackground),
            ],
            startPoint: .top, endPoint: .bottom
        )
        .ignoresSafeArea()
    }

    // MARK: Theme

    /// Brass-amber tint defined in UX-08 §4 — *brass-amber glass = the agent
    /// owns this audio*. Kept as a static so the color is consistent across
    /// every briefing surface even if `AppTheme` is later extended.
    static let brassAmber = Color(red: 0.85, green: 0.60, blue: 0.18)
}

// MARK: - Library row

private struct BriefingsLibraryRow: View {
    let script: BriefingScript

    var body: some View {
        HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
            Image(systemName: "waveform")
                .font(.title3)
                .foregroundStyle(BriefingsView.brassAmber)
                .frame(width: 32)
            VStack(alignment: .leading, spacing: 4) {
                Text(script.title)
                    .font(.headline)
                Text(script.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(relativeDate(script.generatedAt))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            Image(systemName: "play.circle.fill")
                .font(.title2)
                .foregroundStyle(BriefingsView.brassAmber)
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: BriefingsView.brassAmber.opacity(0.12)
        )
    }

    private func relativeDate(_ date: Date) -> String {
        BriefingsLibraryRow.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .full
        return f
    }()
}

// MARK: - Playback context (Identifiable for navigationDestination(item:))

struct BriefingPlaybackContext: Identifiable, Hashable {
    let script: BriefingScript
    var id: UUID { script.id }
}
