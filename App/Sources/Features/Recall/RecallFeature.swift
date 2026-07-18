import Foundation

enum RecallFeature {
    /// One-switch rollback for the temporary Swift proof path. The typed
    /// rendering models remain valid when #69 replaces retrieval in Rust.
    static let isEnabled = true

    @MainActor
    static func start(_ text: String, in session: AgentChatSession) {
        if isEnabled && RecallIntentClassifier.matches(text) {
            session.startRecall(text)
        } else {
            session.startSend(text)
        }
    }
}

enum RecallIntentClassifier {
    private static let phrases = [
        "what did i hear", "what did the", "what did he", "what did she", "what did they",
        "where did i hear", "which episode", "who said", "did i hear",
        "i heard", "remember hearing", "recall", "the one about", "said about",
    ]

    static func matches(_ text: String) -> Bool {
        let normalized = text.lowercased()
        return phrases.contains { normalized.contains($0) }
    }
}
