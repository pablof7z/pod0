import SwiftUI

// MARK: - AdSkipPill
//
// Capsule-shaped indicator + button surfaced in the full-screen
// player when the current playhead falls inside an `AdSegment`. Two
// roles:
//
//   1. Discovery — the user sees "Ad" and learns the section is an
//      ad break (publisher-supplied or detected).
//   2. Manual skip — tapping seeks past `end_secs` even when the
//      auto-skip toggle is off (or when the segment was already
//      auto-skipped earlier in the session and the user scrubbed
//      back; the manual tap is a fresh signal).
//
// Per D7 the view renders only what the snapshot says; the seek
// target comes from the segment's `endSecs` and is dispatched as a
// standard `podcast.player.seek`.

struct AdSkipPill: View {
    @Environment(KernelModel.self) private var model

    let segment: AdSegment

    var body: some View {
        Button(action: skip) {
            HStack(spacing: PodcastSpace.xs) {
                Image(systemName: "forward.end.fill")
                    .font(.system(size: 11, weight: .semibold))
                Text("Skip Ad")
                    .font(PodcastFont.caption.weight(.semibold))
            }
            .padding(.horizontal, PodcastSpace.m)
            .padding(.vertical, PodcastSpace.s)
            .foregroundStyle(.white)
            .background(Color.accentColor, in: Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("ad-skip-pill")
        .accessibilityLabel("Skip ad")
        .accessibilityHint("Seek past the current advertisement.")
    }

    private func skip() {
        model.dispatch(namespace: "podcast.player", body: [
            "op": "seek",
            "position_secs": segment.endSecs,
        ])
    }
}

// MARK: - Lookup helper

extension PlayerState {
    /// Find the active episode's ad segment containing `positionSecs`
    /// in the supplied snapshot library. Returns `nil` when no
    /// segment matches or when the active episode has no annotations.
    func activeAdSegment(in library: [PodcastSummary]) -> AdSegment? {
        guard let episodeId else { return nil }
        let episode = library.flatMap { $0.episodes }.first { $0.id == episodeId }
        guard let segments = episode?.adSegments, !segments.isEmpty else { return nil }
        return segments.first { positionSecs >= $0.startSecs && positionSecs < $0.endSecs }
    }
}
