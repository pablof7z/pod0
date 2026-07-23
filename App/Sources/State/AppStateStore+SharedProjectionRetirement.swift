extension AppStateStore {
    var transcriptReader: any TranscriptReading {
        sharedLibrary?.authoritativeTranscriptReader ?? UnavailableTranscriptReader.shared
    }

    static func hasMigratedNativeState(_ state: AppState) -> Bool {
        state.podcasts.contains { $0.id != Podcast.unknownID }
            || !state.subscriptions.isEmpty
            || !state.episodes.isEmpty
            || !state.notes.isEmpty
            || !state.clips.isEmpty
            || !state.agentMemories.isEmpty
            || state.compiledMemory != nil
            || !state.agentScheduledTasks.isEmpty
            || state.lastPlayedEpisodeID != nil
    }
}
