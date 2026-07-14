import SwiftUI

/// State machine for the full shake → compose → screenshot → annotate → submit flow.
///
/// Phases:
///   idle          — No feedback session active
///   composing     — Sheet open, user typing draft
///   awaitingScreenshot — User dismissed sheet; waiting for shake to capture screen
///   annotating    — Full-screen annotation canvas shown over captured screenshot
@MainActor
@Observable
final class FeedbackWorkflow {
    enum Phase: Equatable {
        case idle
        case composing
        case awaitingScreenshot
        case annotating
    }

    var phase: Phase = .idle
    var draft: String = ""
    var screenshot: UIImage? = nil
    var annotatedImage: UIImage? = nil
    var selectedCategory: FeedbackCategory = .bug

    var isSheetVisible: Bool { phase == .composing }
    var isAnnotationVisible: Bool { phase == .annotating }
}
