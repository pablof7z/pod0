import Foundation

enum TranscriptCredentialWorkflowSignal {
    static let notification = Notification.Name(
        "io.f7z.podcast.transcriptCredentialDidChange"
    )

    static func send() {
        NotificationCenter.default.post(name: notification, object: nil)
        Task { @MainActor in
            WorkflowRuntime.shared.wake()
        }
    }
}
