import Foundation

// MARK: - Ad skip

extension PlaybackState {

    /// Seeks past any ad segment the playhead currently sits inside, when
    /// `autoSkipAdsEnabled` is on. Throttled to one skip per `AdSegment.id`
    /// per playback session via `skippedAdSegmentIDs` — a user who scrubs
    /// back into a previously-skipped ad doesn't get auto-yanked forward a
    /// second time, treating that as a deliberate "let it play" intent.
    ///
    /// No-op when the engine is paused (`time == 0` && `!isPlaying`) — we
    /// shouldn't fight a user who paused inside an ad to copy a URL.
    func applyAutoSkipAdsIfNeeded(at time: TimeInterval) {
        guard autoSkipAdsEnabled, !adSegments.isEmpty else { return }
        // Find the first ad whose `[start, end)` contains the playhead and
        // hasn't been auto-skipped yet this session. Strict half-open
        // intervals so the player can land on `ad.end` after a skip
        // without immediately re-triggering itself.
        guard let segment = adSegments.first(where: { ad in
            time >= ad.start && time < ad.end && !skippedAdSegmentIDs.contains(ad.id)
        }) else { return }
        skippedAdSegmentIDs.insert(segment.id)
        engine.seek(to: segment.end)
    }
}
