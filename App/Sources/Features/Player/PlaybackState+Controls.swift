import Foundation
import Pod0Core

extension PlaybackState {
    func setEpisode(_ newEpisode: Episode) {
        if let sharedCore {
            sharedCore.dispatchPlayback(.select(
                episodeId: EpisodeId(uuid: newEpisode.id),
                segment: nil,
                label: nil
            ))
            return
        }
        let isSameEpisode = episode?.id == newEpisode.id
        if !isSameEpisode {
            onFlushPositions()
            didFireFinishedFor = nil
            lastSnapshotWrite = nil
            skippedAdSegmentIDs = []
            playbackRequested = false
            sessionPolicy.invalidateResumeIntent()
        } else {
            didFireFinishedFor = nil
        }
        episode = newEpisode
        adSegments = newEpisode.adSegments ?? []
        if !isSameEpisode {
            engine.load(newEpisode)
            if newEpisode.playbackPosition > 0 {
                engine.seek(to: newEpisode.playbackPosition)
                recordResumeAttempt(expectedPosition: newEpisode.playbackPosition)
            }
        } else {
            engine.refreshMetadata(for: newEpisode)
            if engine.didReachNaturalEnd {
                let resume = newEpisode.playbackPosition
                let target = resume > 0 && resume < max(0, duration - 5) ? resume : 0
                engine.seek(to: target)
            }
        }
        writeNowPlayingSnapshot(force: true)
        if !isSameEpisode {
            startPersistenceLoop()
            if case .notDownloaded = newEpisode.downloadState {
                onEnsureDownloadEnqueued(newEpisode.id)
            }
        }
    }

    func togglePlayPause() {
        isPlaying ? pause() : play()
    }

    func play() {
        guard let episode else { return }
        if let sharedCore {
            Haptics.medium()
            sharedCore.dispatchPlayback(.play)
            return
        }
        sessionPolicy.invalidateResumeIntent()
        playbackRequested = true
        Haptics.medium()
        if case .failed = engine.state {
            let resume = max(engine.currentTime, episode.playbackPosition)
            engine.load(episode)
            if resume > 0 { engine.seek(to: resume) }
        }
        engine.play()
        if case .failed = engine.state {
            recordPlaybackSignal(name: .playStarted, outcome: .failed)
            playbackRequested = false
            writeNowPlayingSnapshot(force: true)
            return
        }
        recordPlaybackSignal(name: .playStarted, outcome: .succeeded)
        startPersistenceLoop()
        writeNowPlayingSnapshot(force: true)
    }

    func pause() {
        if let sharedCore {
            Haptics.soft()
            sharedCore.dispatchPlayback(.pause)
            return
        }
        playbackRequested = false
        sessionPolicy.invalidateResumeIntent()
        Haptics.soft()
        let pausedEpisodeID = episode?.id
        if engine.didReachNaturalEnd { tickLegacyPersistence() }
        guard episode?.id == pausedEpisodeID else { return }
        engine.pause()
        persistenceTask?.cancel()
        persistenceTask = nil
        onFlushPositions()
        writeNowPlayingSnapshot(force: true)
    }

    func seek(to time: TimeInterval) {
        if let sharedCore {
            sharedCore.dispatchPlayback(.seek(
                positionMilliseconds: Self.coreMilliseconds(time)
            ))
            Haptics.selection()
            return
        }
        engine.seek(to: time)
        Haptics.selection()
        persistAndFlushAfterUserSeek()
    }

    func seekSnapping(to time: TimeInterval) {
        seek(to: time)
    }

    func skipBackward(_ seconds: TimeInterval? = nil) {
        if sharedCore != nil {
            seek(to: max(0, currentTime - (seconds ?? TimeInterval(skipBackwardSeconds))))
        } else {
            engine.skip(back: seconds)
            persistAndFlushAfterUserSeek()
        }
    }

    func skipForward(_ seconds: TimeInterval? = nil) {
        if sharedCore != nil {
            seek(to: min(duration, currentTime + (seconds ?? TimeInterval(skipForwardSeconds))))
        } else {
            engine.skip(forward: seconds)
            persistAndFlushAfterUserSeek()
        }
    }

    func persistAndFlushAfterUserSeek() {
        guard let episode else { return }
        let time = engine.currentTime
        if time > 0 { onPersistPosition(episode.id, time) }
        onFlushPositions()
    }

    func setRate(_ newRate: PlaybackRate) {
        if let sharedCore {
            sharedCore.dispatchPlayback(.setRate(rate: PlaybackRatePermille(
                value: UInt16(clamping: Int((newRate.rawValue * 1_000).rounded()))
            )))
            Haptics.selection()
            return
        }
        engine.setRate(newRate.rawValue)
        Haptics.selection()
    }

    var skipForwardSeconds: Int { Int(engine.skipForwardSeconds) }
    var skipBackwardSeconds: Int { Int(engine.skipBackwardSeconds) }

    func applyPreferences(from settings: Settings) {
        engine.skipForwardSeconds = Double(max(1, settings.skipForwardSeconds))
        engine.skipBackwardSeconds = Double(max(1, settings.skipBackwardSeconds))
        if let sharedCore {
            sharedCore.dispatchPlayback(.setPreferences(
                autoMarkPlayedAtNaturalEnd: settings.autoMarkPlayedAtEnd,
                autoPlayNext: settings.autoPlayNext
            ))
        } else if engine.episode == nil {
            engine.setRate(settings.defaultPlaybackRate)
        }
        autoSkipAdsEnabled = settings.autoSkipAds
        headphoneDoubleTapAction = settings.headphoneDoubleTapAction
        headphoneTripleTapAction = settings.headphoneTripleTapAction
    }

    func setSleepTimer(_ timer: PlaybackSleepTimer) {
        sleepTimer = timer
        if let sharedCore {
            sharedCore.dispatchPlayback(.setSleepTimer(mode: timer.coreValue))
        } else {
            engine.setSleepTimer(timer.engineMode)
        }
        Haptics.selection()
    }
}
