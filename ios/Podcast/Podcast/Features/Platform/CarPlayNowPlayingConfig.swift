import CarPlay
import Foundation
import UIKit

// MARK: - CarPlayNowPlayingConfig
//
// Configures `CPNowPlayingTemplate.shared` — the system-owned template
// CarPlay pushes when audio is playing. Standard transport (play /
// pause / scrubber / skip / artwork) flows through
// `MPRemoteCommandCenter` and `MPNowPlayingInfoCenter`, which the
// `AudioCapability+RemoteCommands` / `AudioCapability+NowPlaying`
// modules already wire. We don't re-route those here.
//
// What this surface owns: any *custom* buttons we want on the Now
// Playing template (speed cycle, chapters, etc.). For the initial
// CarPlay surface we keep the button row empty — the kernel emits
// playback rate / position through standard remote commands, and the
// custom buttons are a follow-up once the kernel projection surfaces
// chapter metadata to iOS.

@MainActor
enum CarPlayNowPlayingConfig {

    /// Wire the standard now-playing template. Called once when the
    /// CarPlay scene installs its root template.
    static func configure(interfaceController: CPInterfaceController) {
        let template = CPNowPlayingTemplate.shared
        template.isAlbumArtistButtonEnabled = false
        // No custom buttons in v1 — see file header. The default
        // template ships with play/pause/scrubber from the remote
        // command center, which is what the driver needs to control
        // playback safely from the head unit.
        template.updateNowPlayingButtons([])
    }
}
