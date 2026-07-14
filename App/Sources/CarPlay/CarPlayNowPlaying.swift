import CarPlay
import Foundation
import UIKit

// MARK: - CarPlayNowPlaying
//
// Configures `CPNowPlayingTemplate.shared` — the system-owned template that
// CarPlay pushes when audio is playing. The standard transport (play / pause /
// skip-30 / scrubber / artwork) flows through `MPRemoteCommandCenter` which
// `NowPlayingCenter.swift` already wires; we don't have to re-route those.
//
// What we add on top: two custom buttons on the now-playing surface.
//   1. **Speed** — a `CPNowPlayingPlaybackRateButton` that cycles through the
//      same `PlaybackRate` ladder the in-app speed sheet uses.
//   2. **Chapters** — a `CPNowPlayingImageButton` that pushes a chapter list
//      template when the live episode has navigable chapters. The button
//      shows / hides itself based on whether chapters are available.
//
// The scene delegate refreshes the chapter button via `refresh(playback:on:)`
// whenever the loaded episode changes — chapters can hydrate post-load, so
// we re-check on each `setEpisode` boundary.

@MainActor
enum CarPlayNowPlaying {

    /// Wire the standard now-playing template's custom button row.
    /// `interfaceController` is needed so the chapter button can push the
    /// chapter list when tapped.
    static func configure(
        playback: PlaybackState,
        interfaceController: CPInterfaceController
    ) {
        let template = CPNowPlayingTemplate.shared
        template.isAlbumArtistButtonEnabled = false
        refresh(playback: playback, interfaceController: interfaceController)
    }

    /// Rebuild the button row. Called after configure, and again whenever
    /// the loaded episode (or its chapter set) changes.
    static func refresh(
        playback: PlaybackState,
        interfaceController: CPInterfaceController
    ) {
        let template = CPNowPlayingTemplate.shared

        var buttons: [CPNowPlayingButton] = []
        buttons.append(makeRateButton(playback: playback))

        if let episode = playback.episode, hasNavigableChapters(episode, playback: playback) {
            buttons.append(makeChaptersButton(playback: playback, on: interfaceController))
        }

        template.updateNowPlayingButtons(buttons)
    }

    // MARK: - Speed cycle button

    private static func makeRateButton(playback: PlaybackState) -> CPNowPlayingButton {
        CPNowPlayingPlaybackRateButton { _ in
            let current = playback.rate
            let next = nextRate(after: current)
            playback.setRate(next)
        }
    }

    /// Cycle through a road-friendly subset of the in-app rates (1, 1.25
    /// equiv, 1.5, 1.75 equiv, 2×). The full ladder has too many steps to
    /// land on the right rate from a single tap while driving.
    private static let cycleLadder: [PlaybackRate] = [
        .normal, .quicker, .fast, .fasterStill, .fastest
    ]

    private static func nextRate(after current: PlaybackRate) -> PlaybackRate {
        if let idx = cycleLadder.firstIndex(of: current) {
            return cycleLadder[(idx + 1) % cycleLadder.count]
        }
        // Snap arbitrary rates back to the nearest cycle entry, then advance.
        let nearest = cycleLadder.min { lhs, rhs in
            abs(lhs.rawValue - current.rawValue) < abs(rhs.rawValue - current.rawValue)
        } ?? .normal
        let idx = cycleLadder.firstIndex(of: nearest) ?? 0
        return cycleLadder[(idx + 1) % cycleLadder.count]
    }

    // MARK: - Chapters button

    private static func hasNavigableChapters(_ episode: Episode, playback: PlaybackState) -> Bool {
        !playback.resolveNavigableChapters(episode).isEmpty
    }

    private static func makeChaptersButton(
        playback: PlaybackState,
        on interfaceController: CPInterfaceController
    ) -> CPNowPlayingButton {
        let glyph = UIImage(systemName: "list.bullet.rectangle")
        return CPNowPlayingImageButton(image: glyph ?? UIImage()) { _ in
            let template = makeChaptersTemplate(playback: playback, on: interfaceController)
            interfaceController.pushTemplate(template, animated: true) { _, _ in }
        }
    }

    /// Build a list template of the live episode's chapters. Tapping a row
    /// seeks the engine to that chapter's start time and pops back to Now
    /// Playing so the driver lands on familiar transport controls.
    private static func makeChaptersTemplate(
        playback: PlaybackState,
        on interfaceController: CPInterfaceController
    ) -> CPListTemplate {
        guard let episode = playback.episode else {
            return CPListTemplate(title: "Chapters", sections: [])
        }
        let chapters = Array(
            playback.resolveNavigableChapters(episode)
                .prefix(CPListTemplate.maximumItemCount)
        )
        let items = chapters.map { chapter -> CPListItem in
            let item = CPListItem(
                text: chapter.title,
                detailText: formatTime(chapter.startTime),
                image: UIImage(systemName: "bookmark")
            )
            item.handler = { _, completion in
                playback.navigationalSeek(to: chapter.startTime)
                interfaceController.popTemplate(animated: true) { _, _ in }
                completion()
            }
            return item
        }
        return CPListTemplate(title: "Chapters", sections: [CPListSection(items: items)])
    }

    private static func formatTime(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds.rounded()))
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 { return String(format: "%d:%02d:%02d", h, m, s) }
        return String(format: "%d:%02d", m, s)
    }
}
