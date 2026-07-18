import Foundation

extension Notification.Name {
    static let workflowJobStoreDidChange = Notification.Name(
        "io.f7z.podcast.workflow.job-store-change"
    )
}

enum WorkflowJobChangeSignal {
    static func post(fileURL: URL) {
        NotificationCenter.default.post(
            name: .workflowJobStoreDidChange,
            object: fileURL.standardizedFileURL.path as NSString
        )
    }

    static func matches(_ notification: Notification, fileURL: URL) -> Bool {
        notification.object as? NSString == fileURL.standardizedFileURL.path as NSString
    }
}
