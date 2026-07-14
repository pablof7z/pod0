import SwiftUI

// MARK: - BriefingRiverView
//
// Lean-back auto-advancing briefing surface ("the river"). UX inspiration:
// Airchat's continuous voice feed, Podimo's daily brief, Snipd's AI DJ.
//
// Architecture is intentionally thin: the view holds an ordered queue of
// `BriefingScript`s and presents one `BriefingPlayerView` at a time with
// `autoplay: true` and `.id(briefingID)`. Each advance simply bumps an index
// and remounts — SwiftUI tears down the prior `BriefingPlayerView` (engine,
// mic controller, `.task`) and rebuilds the next one fresh.
//
// The end-of-stream signal flows: `FakeBriefingPlayerHost`'s item-scoped
// `AVPlayerItemDidPlayToEndTime` observer → host's `onPlaybackEnded` closure
// (wired in `BriefingPlayerEngine.load`) → posts `.briefingPlaybackEnded`
// notification → this view's `.onReceive` advances the index.
//
// Why `.id()` remount instead of refactoring `BriefingPlayerView` to take an
// injected engine? Per advisor: the engine init is microseconds, mic
// controller doesn't request permission until held, `prepareEngine` is two
// awaits. The smoothness win of an injected engine is real but cheap to
// chase later; remount is the safe MVP.
struct BriefingRiverView: View {

    let queue: [BriefingScript]

    @State private var currentIndex: Int = 0

    var body: some View {
        Group {
            if queue.isEmpty {
                emptyState
            } else if currentIndex >= queue.count {
                endOfRiver
            } else {
                BriefingPlayerView(
                    context: BriefingPlaybackContext(script: queue[currentIndex]),
                    autoplay: true
                )
                .id(queue[currentIndex].id)
            }
        }
        .navigationTitle(navTitle)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { riverToolbar }
        .onReceive(NotificationCenter.default.publisher(for: .briefingPlaybackEnded)) { note in
            advance(forBriefingID: note.userInfo?["briefingID"] as? UUID)
        }
    }

    // MARK: - Advance

    /// Only advance when the notification carries the briefing we're
    /// currently presenting. Guards against a stale notification arriving
    /// after a manual skip (toolbar) or after the user backed out and re-
    /// entered the river surface.
    private func advance(forBriefingID firedID: UUID?) {
        guard currentIndex < queue.count else { return }
        if let firedID, firedID != queue[currentIndex].id { return }
        Haptics.medium()
        currentIndex += 1
    }

    // MARK: - Empty / end states

    private var emptyState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Image(systemName: "newspaper")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            Text("No briefings to play")
                .font(.title3.weight(.semibold))
            Text("Compose at least one briefing to start the river.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
        }
        .padding(.vertical, 60)
    }

    private var endOfRiver: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 56))
                .foregroundStyle(BriefingsView.brassAmber)
            Text("That's the river.")
                .font(AppTheme.Typography.largeTitle)
            Text("You're caught up across \(queue.count) briefing\(queue.count == 1 ? "" : "s").")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
            Button {
                Haptics.selection()
                currentIndex = 0
            } label: {
                Label("Play from the top", systemImage: "arrow.counterclockwise")
                    .font(.headline)
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    .padding(.vertical, AppTheme.Spacing.sm)
            }
            .glassSurface(
                cornerRadius: AppTheme.Corner.lg,
                tint: BriefingsView.brassAmber.opacity(0.22),
                interactive: true
            )
            .buttonStyle(.plain)
            .padding(.top, AppTheme.Spacing.sm)
        }
        .padding(.vertical, 60)
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var riverToolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.selection()
                advance(forBriefingID: nil)
            } label: {
                Label("Next", systemImage: "forward.end.fill")
            }
            .disabled(currentIndex >= queue.count)
        }
    }

    // MARK: - Helpers

    private var navTitle: String {
        guard !queue.isEmpty, currentIndex < queue.count else { return "Briefings" }
        return "\(currentIndex + 1) of \(queue.count)"
    }
}
