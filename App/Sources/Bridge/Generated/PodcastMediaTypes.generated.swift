// PodcastMediaTypes.generated.swift
// Media types: agent, voice, TTS, clips.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Voice-mode projection mirroring Rust `VoiceState`.
struct VoiceSnapshot: Equatable {
    var isSpeaking: Bool = false
    var isListening: Bool = false
    var currentRequestId: String? = nil
    var currentVoiceId: String? = nil
    var partialTranscript: String? = nil
    var lastResponse: String? = nil
}

/// Agent-chat conversation surfaced via `PodcastUpdate.agent`.
struct AgentSnapshot: Equatable {
    var messages: [AgentMessageSummary] = []
    /// `true` while the kernel is composing an assistant reply.
    var isBusy: Bool = false
}

/// One row in `AgentSnapshot.messages`.
struct AgentMessageSummary: Identifiable, Equatable, Hashable {
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
    var intentType: String? = nil
    var intentLabel: String? = nil
    var intentDetail: String? = nil
    var schedule: String
    var nextRunAt: Int? = nil
    var lastRunAt: Int? = nil
    /// One of `"pending"`, `"running"`, `"completed"`, `"failed"`.
    var status: String
    var isEnabled: Bool
}

/// One AI agent pick row surfaced via `PodcastUpdate.picks`.
struct AgentPickSummary: Identifiable, Equatable, Hashable {
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
    var transcriptText: String = ""
    var speaker: String? = nil
    var source: String = ""
    var refinementStatus: String = ""
    var createdAt: Int
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false) and Vec fields (omit when empty). Conformance is declared in extensions
// (not struct bodies) so the synthesized memberwise init is preserved.

extension VoiceSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        isSpeaking = try c.decodeIfPresent(Bool.self, forKey: .isSpeaking) ?? false
        isListening = try c.decodeIfPresent(Bool.self, forKey: .isListening) ?? false
        currentRequestId = try c.decodeIfPresent(String.self, forKey: .currentRequestId)
        currentVoiceId = try c.decodeIfPresent(String.self, forKey: .currentVoiceId)
        partialTranscript = try c.decodeIfPresent(String.self, forKey: .partialTranscript)
        lastResponse = try c.decodeIfPresent(String.self, forKey: .lastResponse)
    }
}

extension AgentSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        messages = try c.decodeIfPresent([AgentMessageSummary].self, forKey: .messages) ?? []
        isBusy = try c.decodeIfPresent(Bool.self, forKey: .isBusy) ?? false
    }
}

extension AgentMessageSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        role = try c.decode(String.self, forKey: .role)
        content = try c.decode(String.self, forKey: .content)
        createdAt = try c.decode(Int.self, forKey: .createdAt)
        isGenerating = try c.decodeIfPresent(Bool.self, forKey: .isGenerating) ?? false
    }
}

extension AgentPickSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        episodeTitle = try c.decode(String.self, forKey: .episodeTitle)
        podcastId = try c.decode(String.self, forKey: .podcastId)
        podcastTitle = try c.decode(String.self, forKey: .podcastTitle)
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        publishedAt = try c.decodeIfPresent(Int.self, forKey: .publishedAt) ?? 0
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs)
        pickReason = try c.decodeIfPresent(String.self, forKey: .pickReason) ?? ""
        pickScore = try c.decodeIfPresent(Double.self, forKey: .pickScore) ?? 0
    }
}
