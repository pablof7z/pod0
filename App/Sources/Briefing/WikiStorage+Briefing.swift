import Foundation

// MARK: - WikiStorage ↔ BriefingWikiStorageProtocol

/// Bridges Knowledge's `WikiStorage` (sync, file-backed) to the briefing
/// composer's async lookup surface. Lives in the Briefing module on purpose:
/// `Knowledge/` is layered below `Briefing/` and must not import it.
///
/// Both methods hop off the main actor onto a detached priority-utility task
/// since they read from disk; `WikiStorage` itself is a value-type `Sendable`
/// struct so it crosses the actor boundary safely.
extension WikiStorage: BriefingWikiStorageProtocol {

    func wikiPage(id: UUID) async throws -> WikiPage? {
        let storage = self
        return try await Task.detached(priority: .utility) {
            try storage.allPages().first { $0.id == id }
        }.value
    }

    func wikiPages(matchingTitle titleQuery: String) async throws -> [WikiPage] {
        let q = titleQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return [] }
        let storage = self
        return try await Task.detached(priority: .utility) {
            try storage.allPages()
                // Locale-aware fold so the briefing composer's title
                // lookup matches across Unicode case (Straße / STRASSE,
                // İstanbul / istanbul) — same shape fix the wiki home
                // search and feedback search just got.
                .filter { $0.title.localizedCaseInsensitiveContains(q) }
                .sorted { $0.generatedAt > $1.generatedAt }
        }.value
    }
}
