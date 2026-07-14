import Foundation
import Observation

// MARK: - BriefingPlayerEngine

/// Plays a `BriefingScript` through a `BriefingPlayerHostProtocol` (Lane 1's
/// `AudioEngine`), tracks the current segment + track, and brokers branch
/// events for the *deeper into this* / barge-in flow described in UX-08 §3.
///
/// The engine is `@Observable` so SwiftUI views (the segment rail, transcript
/// pane, attribution chip) update without explicit publishing.
@MainActor
@Observable
final class BriefingPlayerEngine {

    // MARK: Inputs

    /// The script being played. Set once via `load(_:tracks:)`; the engine
    /// does not mutate the script — recorded branches are appended via
    /// `recordBranch(prompt:answer:)` which writes to its own array.
    private(set) var script: BriefingScript?

    /// Ordered tracks the engine is iterating. The composer hands these in
    /// alongside the script so the engine never re-derives them.
    private(set) var tracks: [BriefingTrack] = []

    /// The host that actually plays audio (lock-screen / Now Playing /
    /// CarPlay all live there). Optional so previews can inspect engine state
    /// without standing up an audio host.
    private(set) var host: BriefingPlayerHostProtocol?

    // MARK: Observable state

    /// Index into `tracks` of whatever's currently playing.
    private(set) var currentTrackIndex: Int = 0

    /// `true` when audio is playing (host is producing samples). When a branch
    /// is in progress, this is `false` — branch playback is owned by Lane 6's
    /// voice mode, not us.
    private(set) var isPlaying: Bool = false

    /// The segment whose track is active. Computed from `currentTrackIndex`;
    /// surfaced as a stored property for SwiftUI view ergonomics.
    private(set) var activeSegmentID: UUID?

    /// Branches recorded during this playback session. These get persisted
    /// when `flushRecordedBranchesToScript()` runs at session end.
    private(set) var sessionBranches: [BriefingBranch] = []

    /// The branch event stream. SwiftUI views subscribe via `.task { for await
    /// e in engine.branchEvents { … } }` to drive the W4 lift-card chrome and
    /// hand the prompt to Lane 6's voice mode.
    let branchEvents: AsyncStream<BranchEvent>
    private let branchContinuation: AsyncStream<BranchEvent>.Continuation

    /// The position the main thread is paused at when a branch begins. Sample-
    /// accurate so the *pause-and-resume* contract holds.
    private(set) var branchPauseAnchorSeconds: TimeInterval = 0

    // MARK: Init

    init(host: BriefingPlayerHostProtocol? = nil) {
        self.host = host
        var continuation: AsyncStream<BranchEvent>.Continuation!
        self.branchEvents = AsyncStream { continuation = $0 }
        self.branchContinuation = continuation
    }

    deinit {
        branchContinuation.finish()
    }

    // MARK: Loading

    /// Loads a script + its tracks for playback. Replaces any prior session.
    ///
    /// Wires the host's `onPlaybackEnded` to flip `isPlaying = false` and
    /// post the `.briefingPlaybackEnded` notification — `BriefingRiverView`
    /// observes that to auto-advance to the next briefing in the queue.
    func load(
        _ script: BriefingScript,
        tracks: [BriefingTrack],
        host: BriefingPlayerHostProtocol? = nil
    ) {
        self.script = script
        self.tracks = tracks
        if let host { self.host = host }
        currentTrackIndex = 0
        sessionBranches = []
        activeSegmentID = tracks.first?.segmentID
        isPlaying = false
        // Capture the loaded script's id so the notification carries the
        // briefing that actually ended (re-loads during a session would
        // otherwise let stale closures post the wrong id).
        let scriptID = script.id
        self.host?.onPlaybackEnded = { [weak self] in
            self?.isPlaying = false
            NotificationCenter.default.post(
                name: .briefingPlaybackEnded,
                object: nil,
                userInfo: ["briefingID": scriptID]
            )
        }
    }

    // MARK: Transport

    /// Begins playback from the current track, at the current host position.
    /// The host owns the actual asset URL — we hand it the stitched .m4a and
    /// the absolute time inside that file, computed from cumulative durations.
    func play(stitchedURL: URL) async {
        guard !tracks.isEmpty else { return }
        // Make sure the shared audio session is in `.briefingPlayback` so
        // lock-screen / Now-Playing / CarPlay all light up correctly. Idempotent
        // when already in that mode.
        try? AudioSessionCoordinator.shared.activate(.briefingPlayback)
        let positionSeconds = absoluteSecondsForCurrentTrack()
        await host?.play(assetURL: stitchedURL, startAt: positionSeconds)
        isPlaying = true
    }

    func pause() async {
        await host?.pause()
        isPlaying = false
    }

    func resume() async {
        // Resume always lands in `.briefingPlayback` — even if we got here
        // straight out of a barge-in turn that was using `.duckedForVoice`.
        try? AudioSessionCoordinator.shared.activate(.briefingPlayback)
        await host?.resume()
        isPlaying = true
    }

    /// Skip the current segment (UX-08 §5 — *swipe-down on the active card*).
    /// Advances `currentTrackIndex` past every track belonging to the active
    /// segment, then asks the host to seek to the new boundary.
    func skipCurrentSegment() async {
        guard let segmentID = activeSegmentID else { return }
        var idx = currentTrackIndex
        while idx < tracks.count, tracks[idx].segmentID == segmentID {
            idx += 1
        }
        await advance(to: idx)
    }

    /// Jump directly to a segment by id (rail tap). The engine seeks the host
    /// to the segment's first track boundary.
    func jump(toSegment segmentID: UUID) async {
        guard let idx = tracks.firstIndex(where: { $0.segmentID == segmentID }) else { return }
        await advance(to: idx)
    }

    // MARK: Branching

    /// Begin a branch — UX-08 §5 *Hold-to-pause-and-ask* / *↳ deeper*. The
    /// engine pauses, anchors the main thread at the host's current position,
    /// and emits a `.began` event. Lane 6's voice mode answers the prompt
    /// and calls `endBranch(answerText:)` on completion.
    func beginBranch(prompt: String) async {
        await pause()
        branchPauseAnchorSeconds = host?.currentTimeSeconds ?? absoluteSecondsForCurrentTrack()
        let event = BranchEvent.began(
            prompt: prompt,
            atSeconds: branchPauseAnchorSeconds,
            segmentID: activeSegmentID
        )
        branchContinuation.yield(event)
    }

    /// Conclude a branch — record it, emit `.ended`, resume from the anchor.
    func endBranch(prompt: String, answerText: String) async {
        guard let segmentID = activeSegmentID else { return }
        let branch = BriefingBranch(
            parentSegmentID: segmentID,
            pausedAtSeconds: branchPauseAnchorSeconds,
            prompt: prompt,
            answerText: answerText
        )
        sessionBranches.append(branch)
        branchContinuation.yield(.ended(branch: branch))
        await host?.seek(to: branchPauseAnchorSeconds)
        await resume()
    }

    /// Returns the recorded branches merged into the script for persistence.
    /// Caller is responsible for handing the result back to `BriefingStorage`.
    func flushRecordedBranchesToScript() -> BriefingScript? {
        guard var s = script else { return nil }
        s.recordedBranches.append(contentsOf: sessionBranches)
        sessionBranches = []
        script = s
        return s
    }

    // MARK: - Helpers

    private func advance(to index: Int) async {
        currentTrackIndex = min(max(0, index), tracks.count)
        activeSegmentID = tracks[safe: currentTrackIndex]?.segmentID
        let position = absoluteSecondsForCurrentTrack()
        await host?.seek(to: position)
    }

    /// Cumulative seconds of every track that comes before `currentTrackIndex`.
    /// This is the `startAt` value the host expects when playing the stitched
    /// .m4a — track[N] begins at the sum of durations of tracks[0..<N].
    private func absoluteSecondsForCurrentTrack() -> TimeInterval {
        guard currentTrackIndex > 0 else { return 0 }
        return tracks
            .prefix(currentTrackIndex)
            .reduce(0.0) { $0 + $1.durationSeconds }
    }

    // MARK: - BranchEvent

    enum BranchEvent: Sendable, Hashable {
        case began(prompt: String, atSeconds: TimeInterval, segmentID: UUID?)
        case ended(branch: BriefingBranch)
    }
}

// MARK: - Notification

extension Notification.Name {
    /// Fired by `BriefingPlayerEngine` when the host signals end-of-stream
    /// for the loaded briefing. `userInfo["briefingID"]` carries the
    /// `BriefingScript.id` that just ended so the river can verify it's
    /// advancing past the briefing it thinks it is.
    static let briefingPlaybackEnded = Notification.Name("io.f7z.podcast.briefingPlaybackEnded")
}

// MARK: - Array safe subscript

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
