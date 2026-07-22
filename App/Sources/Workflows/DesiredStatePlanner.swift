import Foundation

/// Stable native identities retained by other migration seams. Scheduled-run
/// planning itself is Rust-owned; Swift may only decode the legacy identity
/// while importing old JobStore rows.
enum DesiredStatePlanner {
    static func audioVersion(_ episode: Episode) -> String {
        ArtifactRepository.version(parts: [
            episode.enclosureURL.absoluteString,
            episode.enclosureMimeType ?? "",
            String(episode.duration ?? 0),
        ])
    }

    static func scheduledOccurrenceID(taskID: UUID, scheduledFor: Date) -> String {
        "scheduled:\(taskID.uuidString):\(Int(scheduledFor.timeIntervalSince1970))"
    }
}
