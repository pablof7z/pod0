import SwiftUI

/// Top-level "Ask" tab — hosts the AI agent chat as a full-screen surface.
/// Wraps `AgentChatView` (formerly presented as a sheet) so it lives inline
/// in the tab bar.
struct AskAgentView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @Environment(AgentAskCoordinator.self) private var askCoordinator
    @State private var session: AgentChatSession?

    var body: some View {
        if let session {
            AgentChatView(session: session)
        } else {
            Color.clear
                .onAppear {
                    session = AgentChatSession(
                        store: store,
                        playback: playback,
                        askCoordinator: askCoordinator
                    )
                }
        }
    }
}
