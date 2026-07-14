import SwiftUI

// MARK: - HoldToClipGestureModifier

/// Wraps a transcript sentence with the 600ms hold-to-clip gesture defined in
/// UX-03 §6.4. Rising haptics fire at 200/400/600ms (`light` → `medium` →
/// `heavy`). Release before 600ms cancels with no haptic. On commit the
/// modifier presents `ClipComposerSheet` pre-populated with the gesture's
/// sentence range — sheet ownership lives at the row level (per-row state)
/// so the existing `TranscriptReaderView` doesn't need new callback wiring.
///
/// Concurrency note: the timer task captures only `Sendable` values
/// (`Segment`, `Episode`, `Transcript`) and hops to `@MainActor` for the
/// haptic + state writes. Cancelling the task on release / disappear stops
/// further haptics and re-arms the gesture cleanly.
struct HoldToClipGestureModifier: ViewModifier {

    // MARK: Inputs

    let episode: Episode
    let transcript: Transcript
    let segment: Segment

    // MARK: Internal state

    @State private var holdTask: Task<Void, Never>?
    @State private var isHolding = false
    @State private var composerSegment: Segment?

    // MARK: Body

    func body(content: Content) -> some View {
        content
            .scaleEffect(isHolding ? 1.02 : 1.0)
            .animation(.easeOut(duration: 0.18), value: isHolding)
            .simultaneousGesture(holdGesture)
            .onDisappear { cancelHold() }
            .sheet(item: $composerSegment) { seg in
                ClipComposerSheet(
                    episode: episode,
                    transcript: transcript,
                    initialSegment: seg
                )
            }
    }

    // MARK: - Gesture

    /// `DragGesture(minimumDistance: 0)` lets us track press-down without
    /// shadowing the tap or the existing 300ms long-press on the row. We
    /// drive the rising-haptic ladder from a `Task` rather than the
    /// gesture's own timing so the three beats fire on a precise schedule
    /// even if SwiftUI's gesture coalescing skews `onChanged` callbacks.
    private var holdGesture: some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { _ in
                guard !isHolding else { return }
                isHolding = true
                holdTask?.cancel()
                holdTask = Task { @MainActor in
                    do {
                        try await Task.sleep(for: .milliseconds(200))
                        Haptics.light()
                        try await Task.sleep(for: .milliseconds(200))
                        Haptics.medium()
                        try await Task.sleep(for: .milliseconds(200))
                        // 600ms reached — commit. Heavy impact rounds out the
                        // rising envelope (light → medium → heavy) per
                        // UX-03 §6.4.
                        Haptics.heavy()
                        composerSegment = segment
                    } catch {
                        // Cancelled before 600ms — silent abort.
                    }
                    isHolding = false
                }
            }
            .onEnded { _ in cancelHold() }
    }

    private func cancelHold() {
        holdTask?.cancel()
        holdTask = nil
        isHolding = false
    }
}

// MARK: - View ergonomics

extension View {
    /// Applies `HoldToClipGestureModifier` to a transcript sentence row. The
    /// composer sheet is hosted by the modifier itself; callers don't need
    /// to manage state.
    func holdToClip(
        episode: Episode,
        transcript: Transcript,
        segment: Segment
    ) -> some View {
        modifier(HoldToClipGestureModifier(
            episode: episode,
            transcript: transcript,
            segment: segment
        ))
    }
}
