import SwiftUI

private struct ChapterProjectionScopeModifier: ViewModifier {
    @Environment(AppStateStore.self) private var store
    let episodeID: UUID

    func body(content: Content) -> some View {
        content
            .onAppear {
                store.sharedLibrary?.openChapterProjection(episodeID: episodeID)
            }
            .onDisappear {
                store.sharedLibrary?.closeChapterProjection(episodeID: episodeID)
            }
    }
}

extension View {
    func chapterProjectionScope(episodeID: UUID) -> some View {
        modifier(ChapterProjectionScopeModifier(episodeID: episodeID))
    }
}
