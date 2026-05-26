// PodcastMediaTypes.generated.swift
// Media types: agent, voice, briefing, TTS, clips.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Voice-mode projection mirroring Rust `VoiceState`.
struct VoiceSnapshot: Codable, Equatable {
    var isSpeaking: Bool = false
    var isListening: Bool = false
    var currentRequestId: String? = nil
    var currentVoiceId: String? = nil
    var partialTranscript: String? = nil
    var lastResponse: String? = nil
}

/// Agent-chat conversation surfaced via `PodcastUpdate.agent`.
struct AgentSnapshot: Codable, Equatable {
    var messages: [AgentMessageSummary] = []
    /// `true` while the kernel is composing an assistant reply.
    var isBusy: Bool = false
}

/// One row in `AgentSnapshot.messages`.
struct AgentMessageSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    /// `"user"` or `"assistant"`.
    var role: String
    var content: String
    var createdAt: Int
    var isGenerating: Bool = false
}

/// One agent-scheduled task surfaced via `PodcastUpdate.agentTasks`.
struct AgentTaskSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var description: String? = nil
    var actionNamespace: String
    var actionBody: String
    var schedule: String
    var nextRunAt: Int? = nil
    var lastRunAt: Int? = nil
    /// One of `"pending"`, `"running"`, `"completed"`, `"failed"`.
    var status: String
    var isEnabled: Bool
}

/// One AI agent pick row surfaced via `PodcastUpdate.picks`.
struct AgentPickSummary: Codable, Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastId: String
    var podcastTitle: String
    var artworkUrl: String? = nil
    var publishedAt: Int = 0
    var durationSecs: Double? = nil
    var pickReason: String = ""
    var pickScore: Double = 0

    var id: String { episodeId }
}

/// Daily briefing projection — mirrors `BriefingSnapshot` in Rust.
struct BriefingSnapshot: Codable, Equatable, Hashable {
    /// One of `"pending"`, `"generating"`, `"ready"`, `"delivered"`, `"failed"`.
    var status: String = "pending"
    var isGenerating: Bool = false
    var segmentCount: Int = 0
    var segments: [BriefingSegmentSummary] = []
    var lastGeneratedAt: Int? = nil
    var nextScheduledMinutes: Int? = nil
}

/// One row in `BriefingSnapshot.segments`.
struct BriefingSegmentSummary: Codable, Equatable, Hashable, Identifiable {
    var kind: String
    var text: String
    var podcastTitle: String? = nil
    var episodeTitle: String? = nil

    var id: String { "\(kind)|\(text.prefix(40))" }
}

/// One agent-generated TTS episode row surfaced via `PodcastUpdate.ttsEpisodes`.
struct TtsEpisodeSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var script: String
    var durationEstimateSecs: Double
    var createdAt: Int
    var status: String
    var voiceId: String? = nil
}

/// User-saved audio clip from an episode.
struct ClipSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var episodeId: String
    var episodeTitle: String
    var podcastTitle: String
    var startSecs: Double
    var endSecs: Double
    var title: String? = nil
    var createdAt: Int
}
