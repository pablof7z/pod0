// PodcastTypes.generated.swift
// This file has been split into four focused files in the same directory:
//
//   PodcastUpdate.generated.swift      — PodcastUpdate, PlayerState, SettingsSnapshot,
//                                        AccountSummary, DownloadQueueSnapshot,
//                                        DownloadItemSnapshot
//   PodcastLibraryTypes.generated.swift — PodcastSummary, EpisodeSummary, ChapterSummary,
//                                        TranscriptEntry, AdSegment, OwnedPodcastInfo,
//                                        NostrShowSummary
//   PodcastMediaTypes.generated.swift  — VoiceSnapshot, AgentSnapshot, AgentMessageSummary,
//                                        AgentTaskSummary, AgentPickSummary,
//                                        BriefingSnapshot, BriefingSegmentSummary,
//                                        TtsEpisodeSummary, ClipSummary
//   PodcastSocialTypes.generated.swift — InboxItem, CommentSummary, ContactSummary,
//                                        SocialSnapshot, CategoryBrowseItem, WikiArticle,
//                                        KnowledgeSearchResult, MemoryFact
//
// Intended regeneration command (once the dumper exists):
//
//   cargo run -p nmp-app-podcast --features codegen-schema \
//       --bin dump_projection_schemas \
//     | cargo run -p nmp-codegen -- gen swift
//
// Source of truth: apps/nmp-app-podcast/src/ffi/snapshot.rs
