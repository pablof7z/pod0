// BridgeSendable.swift
// Retroactive `Sendable` conformances for the podcast projection mirrors.
//
// These are immutable, value-type snapshots decoded once from the Rust kernel
// and never mutated after construction, so they are safe to cross actor
// isolation boundaries. `KernelModel.applyPodcastUpdate` offloads the O(N×M)
// `libraryMetaHash` / `snapshotContentHash` computation to a background
// `Task.detached`, which requires the captured `PodcastUpdate` (and everything
// it transitively contains) to be `Sendable`.
//
// Declared here rather than on the generated structs themselves so they survive
// the codegen pipeline clobbering `*.generated.swift`. Swift requires *checked*
// `Sendable` conformance to live in the type's own source file, so a retroactive
// conformance in this separate file must be `@unchecked`. That is sound here:
// every type below is a `struct`/`enum` whose stored properties are value types
// (String / Int / Double / Bool / Date / arrays / other types in this list).
// There are no reference-type members. Delete this file once the generator emits
// `Sendable` directly on the generated types.

import Foundation

// MARK: - Property-wrapper conditional conformances

extension CodableDefault: @unchecked Sendable where Source.Value: Sendable {}
extension DefaultEmptyArray: @unchecked Sendable where Element: Sendable {}
extension DefaultSettings: @unchecked Sendable {}

// MARK: - Default sources (zero stored state)

extension BoolFalse: @unchecked Sendable {}
extension EmptyStringArray: @unchecked Sendable {}

// MARK: - Projection snapshot types

extension PodcastUpdate: @unchecked Sendable {}
extension AppRelayRow: @unchecked Sendable {}
extension PlayerState: @unchecked Sendable {}
extension AccountSummary: @unchecked Sendable {}
extension AdSegment: @unchecked Sendable {}
extension AgentContextEpisode: @unchecked Sendable {}
extension AgentContextSnapshot: @unchecked Sendable {}
extension AgentMessageSummary: @unchecked Sendable {}
extension AgentPickSummary: @unchecked Sendable {}
extension AgentSnapshot: @unchecked Sendable {}
extension AgentTaskSummary: @unchecked Sendable {}
extension CategoryBrowseItem: @unchecked Sendable {}
extension ChapterSummary: @unchecked Sendable {}
extension ClipSummary: @unchecked Sendable {}
extension CommentSummary: @unchecked Sendable {}
extension ContactSummary: @unchecked Sendable {}
extension DownloadItemSnapshot: @unchecked Sendable {}
extension DownloadQueueSnapshot: @unchecked Sendable {}
extension EpisodeSummary: @unchecked Sendable {}
extension InboxItem: @unchecked Sendable {}
extension KnowledgeSearchResult: @unchecked Sendable {}
extension MemoryFact: @unchecked Sendable {}
extension NostrShowSummary: @unchecked Sendable {}
extension OwnedPodcastInfo: @unchecked Sendable {}
extension PodcastSummary: @unchecked Sendable {}
extension SettingsSnapshot: @unchecked Sendable {}
extension SocialSnapshot: @unchecked Sendable {}
extension TranscriptEntry: @unchecked Sendable {}
extension TtsEpisodeSummary: @unchecked Sendable {}
extension VoiceSnapshot: @unchecked Sendable {}
