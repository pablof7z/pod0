import Foundation
import os.log

// MARK: - SleepTimer

/// Drives sleep-timer behavior independent of the audio engine — three modes:
///
/// 1. **Duration**: fires after N minutes; fades volume over the last 8s, then
///    pauses playback.
/// 2. **End-of-episode**: arms a flag; engine asks `shouldStopAtEnd()` when
///    `AVPlayerItem.didPlayToEndTimeNotification` fires.
/// 3. **Shake-to-extend**: `extend(by:)` shifts the deadline forward — the
///    integration with the existing `ShakeDetector` (`Design/ShakeDetector.swift`)
///    happens in the player view (Lane 4). This timer just publishes the API.
///
/// The timer publishes `phase` so the player UI can render a countdown.
@MainActor
@Observable
final class SleepTimer {

    // MARK: - Mode

    enum Mode: Equatable, Sendable {
        case off
        case duration(TimeInterval)        // total seconds
        case endOfEpisode
    }

    enum Phase: Equatable, Sendable {
        case idle
        case armed(remaining: TimeInterval)  // duration mode countdown
        case armedEndOfEpisode
        case fading(remaining: TimeInterval) // last few seconds, fade-out engaged
        case fired
    }

    // MARK: - Hooks

    /// Called when the timer wants the engine to start fading volume.
    /// Receives a 0…1 multiplier per tick; engine maps to `AVPlayer.volume`.
    var onFadeTick: (Float) -> Void = { _ in }

    /// Called when the timer fires — the engine should pause.
    var onFire: () -> Void = {}

    // MARK: - State

    private(set) var mode: Mode = .off
    private(set) var phase: Phase = .idle

    // MARK: - Tunables

    /// Length of the volume fade ramp before pause. 8 s matches Overcast.
    let fadeDurationSeconds: TimeInterval = 8

    // MARK: - Private

    private let logger = Logger.app("SleepTimer")
    private var deadline: Date?
    private var tickTask: Task<Void, Never>?

    // MARK: - Public API

    /// Arm the timer. Cancels any previous arming.
    func set(_ mode: Mode) {
        cancel()
        self.mode = mode
        switch mode {
        case .off:
            phase = .idle
        case .duration(let seconds):
            deadline = Date().addingTimeInterval(seconds)
            phase = .armed(remaining: seconds)
            startTicking()
        case .endOfEpisode:
            phase = .armedEndOfEpisode
        }
    }

    /// Push the deadline forward (shake-to-extend). No-op outside duration mode.
    func extend(by seconds: TimeInterval) {
        guard case .duration = mode, let current = deadline else { return }
        deadline = current.addingTimeInterval(seconds)
        // Restore full volume in case we were already fading.
        onFadeTick(1.0)
        if case .fading(let remaining) = phase {
            phase = .armed(remaining: remaining + seconds)
        } else if case .armed(let remaining) = phase {
            phase = .armed(remaining: remaining + seconds)
        }
        // Restart the tick task to re-evaluate state.
        startTicking()
    }

    /// Cancel any active timer. Safe to call from any state.
    func cancel() {
        tickTask?.cancel()
        tickTask = nil
        deadline = nil
        mode = .off
        phase = .idle
        onFadeTick(1.0) // restore full volume
    }

    /// Engine calls this when an episode finishes — returns `true` if the
    /// "end of episode" mode was armed and the engine should stay paused.
    func shouldStopAtEpisodeEnd() -> Bool {
        if case .endOfEpisode = mode {
            phase = .fired
            mode = .off
            onFire()
            return true
        }
        return false
    }

    // MARK: - Tick loop

    private func startTicking() {
        tickTask?.cancel()
        tickTask = Task { @MainActor [weak self] in
            await self?.tickLoop()
        }
    }

    private func tickLoop() async {
        while !Task.isCancelled {
            guard let deadline else { return }
            let remaining = deadline.timeIntervalSinceNow

            if remaining <= 0 {
                phase = .fired
                mode = .off
                self.deadline = nil
                onFadeTick(0.0)
                onFire()
                return
            }

            if remaining <= fadeDurationSeconds {
                let t = Float(remaining / fadeDurationSeconds) // 1 → 0
                phase = .fading(remaining: remaining)
                onFadeTick(max(0.0, min(1.0, t)))
            } else {
                phase = .armed(remaining: remaining)
                onFadeTick(1.0)
            }

            // 250ms tick — smooth enough for a fade ramp without hogging CPU.
            try? await Task.sleep(nanoseconds: 250_000_000)
        }
    }
}
