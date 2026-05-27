import Foundation
import MediaPlayer

// MARK: - MPRemoteCommandCenter integration
//
// The lock-screen play button, AirPods double-tap, CarPlay buttons, and
// any other system-driven remote command all route through
// `MPRemoteCommandCenter.shared()`. The executor registers handlers that
// translate each tap into a direct `AudioCommand` execution — and
// nothing more.
//
// D7 — *never* decide. The handlers here only cover taps where the
// command-to-execute is unambiguous from the system event:
//
//   - play             → `AudioCommand.play`
//   - pause            → `AudioCommand.pause`
//   - togglePlayPause  → `play` / `pause` based on `timeControlStatus`
//   - changePlaybackPosition → `AudioCommand.seek(positionSecs: <event>)`
//                              (the system event carries the absolute
//                               target; the capability does *not*
//                               compute it)
//
// Skip-forward / skip-backward are **deliberately omitted** at this
// milestone. A skip interval is a Rust policy decision (chapter snap,
// smart-skip, user-configurable forward/back, episode boundary). The
// canonical spec §5.1 ships a `RemoteCommand{kind: .skipForward(secs)}`
// report variant so the executor can forward the tap to Rust without
// computing the new playhead in Swift; the M3.A skeleton does not yet
// ship that variant. Skip wiring lands when the canonical
// `RemoteCommand` shape migrates (BACKLOG entry: skip-remote-command).

@MainActor
extension AudioCapability {

    /// Idempotent. Wires the play / pause / toggle / scrub handlers.
    /// Called from `AudioCapability.start()`.
    func installRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()

        // Play / Pause / Toggle.
        center.playCommand.removeTarget(nil)
        center.playCommand.addTarget { [weak self] _ in
            guard let self else { return .commandFailed }
            return MainActor.assumeIsolated {
                self.execute(.play)
                return .success
            }
        }
        center.pauseCommand.removeTarget(nil)
        center.pauseCommand.addTarget { [weak self] _ in
            guard let self else { return .commandFailed }
            return MainActor.assumeIsolated {
                self.execute(.pause)
                return .success
            }
        }
        center.togglePlayPauseCommand.removeTarget(nil)
        center.togglePlayPauseCommand.addTarget { [weak self] _ in
            guard let self else { return .commandFailed }
            return MainActor.assumeIsolated {
                let isPlaying = self.avPlayer.timeControlStatus == .playing
                self.execute(isPlaying ? .pause : .play)
                return .success
            }
        }

        // Scrub — the system event carries an absolute target position
        // in seconds, so this is *not* a Swift-side decision. The
        // executor forwards it as a `Seek` command and lets the
        // follow-up `Playing` / `Paused` report tell Rust where the
        // playhead actually landed.
        center.changePlaybackPositionCommand.removeTarget(nil)
        center.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard
                let self,
                let positionEvent = event as? MPChangePlaybackPositionCommandEvent
            else { return .commandFailed }
            return MainActor.assumeIsolated {
                self.execute(.seek(positionSecs: positionEvent.positionTime))
                return .success
            }
        }
    }

    /// Idempotent. Detaches every handler this executor installed.
    /// Called from `AudioCapability.stop()`.
    func removeRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()
        center.playCommand.removeTarget(nil)
        center.pauseCommand.removeTarget(nil)
        center.togglePlayPauseCommand.removeTarget(nil)
        center.changePlaybackPositionCommand.removeTarget(nil)
    }
}
