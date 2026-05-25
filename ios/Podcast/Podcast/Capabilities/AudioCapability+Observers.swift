import AVFoundation
import Foundation

// MARK: - AVPlayer / AVPlayerItem observer wiring
//
// Periodic time observer + KVO observers + end-of-item NotificationCenter
// hook. Split out of `AudioCapability.swift` so the main file stays
// focused on command dispatch and the executor lifecycle (300-line soft
// limit, AGENTS.md).
//
// All observers funnel into the `emit(_:)` helper on the main class so
// every state change crosses the FFI boundary as an `AudioReport`.
// D7 holds at every callback site.

@MainActor
extension AudioCapability {

    // MARK: Time observer

    /// Idempotent. Installs the 1 Hz periodic time observer; subsequent
    /// `Load` calls reuse the same observer (it tracks the player, not
    /// the item).
    func installTimeObserverIfNeeded() {
        if hasTimeObserver { return }
        // D8: ≤1 Hz position reports. `preferredTimescale = 600` covers
        // 30 fps and 25 fps cleanly.
        let interval = CMTime(seconds: 1, preferredTimescale: 600)
        let token = avPlayer.addPeriodicTimeObserver(
            forInterval: interval,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.onTimeTick()
            }
        }
        setTimeObserverToken(token)
    }

    /// Only emit a `Playing` tick when AVPlayer is actually playing — a
    /// periodic observer fires once just after `replaceCurrentItem` and
    /// would otherwise leak a `position=0` report on every load.
    func onTimeTick() {
        guard avPlayer.timeControlStatus == .playing else { return }
        emitReport(.playing(
            url: currentTrackURL ?? "",
            positionSecs: currentPosition(),
            durationSecs: currentDuration()))
        updateNowPlayingElapsed()
    }

    // MARK: Item observers

    func installItemObservers(for item: AVPlayerItem) {
        let status = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            Task { @MainActor in self?.onItemStatusChange(item) }
        }
        let loaded = item.observe(\.loadedTimeRanges, options: [.new]) {
            [weak self] item, _ in
            Task { @MainActor in self?.onLoadedTimeRangesChange(item) }
        }
        let end = NotificationCenter.default.addObserver(
            forName: AVPlayerItem.didPlayToEndTimeNotification,
            object: item,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.onItemEnd() }
        }
        setItemObservers(status: status, loadedRanges: loaded, end: end)
    }

    func teardownItemObservers() {
        let (status, loaded, end) = clearItemObservers()
        status?.invalidate()
        loaded?.invalidate()
        if let end {
            NotificationCenter.default.removeObserver(end)
        }
    }

    // MARK: Handlers

    func onItemStatusChange(_ item: AVPlayerItem) {
        switch item.status {
        case .failed:
            let msg = item.error?.localizedDescription ?? "playback failed"
            emitReport(.failed(url: currentTrackURL ?? "", error: msg))
        case .readyToPlay, .unknown:
            // ReadyToPlay isn't itself a report — `Playing` lands once
            // playback actually starts and the time observer ticks.
            break
        @unknown default:
            break
        }
    }

    func onLoadedTimeRangesChange(_ item: AVPlayerItem) {
        let duration = currentDuration()
        guard duration > 0 else { return }
        // Furthest loaded-ahead time over all ranges, divided by item duration.
        let loadedSecs: Double = item.loadedTimeRanges.reduce(0) { acc, value in
            let range = value.timeRangeValue
            let end = (range.start + range.duration).seconds
            return max(acc, end.isFinite ? end : acc)
        }
        let fraction = Float(min(1.0, max(0.0, loadedSecs / duration)))
        // Only emit when the fraction moves by ≥1 % — avoids report
        // floods on short items.
        if abs(fraction - lastBufferedFractionStorage) >= 0.01 {
            setLastBufferedFraction(fraction)
            emitReport(.bufferingProgress(fraction: fraction))
        }
    }

    func onItemEnd() {
        // D7: capability reports, never decides. End-of-item is a
        // `Stopped` report; what happens next (auto-advance, mark as
        // played) is the player actor's call.
        //
        // `emitReport(.stopped)` folds through `updateNowPlayingForReport`
        // which clears the lock-screen dictionary; no separate
        // `clearNowPlaying()` call needed.
        emitReport(.stopped)
    }
}
