import Foundation

extension PlaybackState {
    /// Live label for the sleep-timer action chip. Read from SwiftUI so the
    /// engine's per-tick phase changes remain observable.
    var sleepTimerChipLabel: String {
        switch engine.sleepTimer.phase {
        case .idle:
            return "Sleep"
        case .armed(let remaining), .fading(let remaining):
            return Self.formatSleepTimerRemaining(remaining)
        case .armedEndOfEpisode:
            return "End"
        case .fired:
            return "Sleep"
        }
    }

    private static func formatSleepTimerRemaining(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds.rounded(.up)))
        let hours = total / 3_600
        let minutes = (total % 3_600) / 60
        let remainingSeconds = total % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, remainingSeconds)
            : String(format: "%d:%02d", minutes, remainingSeconds)
    }
}
