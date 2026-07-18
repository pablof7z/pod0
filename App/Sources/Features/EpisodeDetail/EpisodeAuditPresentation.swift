import Foundation

enum EpisodeAuditPresentation {
    static func summary(for event: EpisodeAuditEvent) -> String {
        switch event.kind {
        case .downloadFailed:
            return "The download stopped safely."
        case .transcriptPublisherFailed:
            return "The publisher transcript could not be used."
        case .transcriptFailed:
            return "Transcription stopped safely."
        case .transcriptIndexFailed:
            return "The transcript is ready, but search indexing stopped."
        default:
            return event.summary
        }
    }

    static func details(for event: EpisodeAuditEvent) -> [EpisodeAuditEvent.Detail] {
        event.details.compactMap { detail in
            switch detail.label.lowercased() {
            case "http status", "error domain", "error code", "mime", "bytes",
                 "resume data", "resume data saved", "stage", "cellular allowed":
                return detail
            case "url":
                guard let host = URL(string: detail.value)?.host else { return nil }
                return .init("Host", host)
            default:
                return nil
            }
        }
    }
}
