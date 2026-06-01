import Foundation
import os

// MARK: - One-shot pre-#215 synthetic-episode backfill
//
// Extracted from `AppStateStore+KernelProjection.swift` to keep that file
// under the 500-line hard limit (AGENTS.md), per the `kernelprojection-split`
// backlog item. Called synchronously from `attachKernel` before the
// observation Task's first `applyKernelState`.

extension AppStateStore {

    /// Dedicated logger for the backfill path. `AppStateStore.logger` is
    /// `private` to its defining file, so this extension declares its own.
    private static let backfillLogger = Logger.app("SyntheticEpisodeBackfill")

    /// `UserDefaults` key gating the one-shot pre-#215 synthetic-episode
    /// backfill. Set `true` only after a backfill pass completes, so a pass
    /// interrupted by a crash or early termination retries on the next launch
    /// rather than silently leaving episodes stranded in the Swift-only store.
    private static let syntheticBackfillDoneKey =
        "synthetic_episode_backfill_v1_done"

    /// One-shot migration: re-register agent-generated episodes that predate
    /// PR #215 (`kernelRegisterSyntheticEpisode`) into the Rust kernel store.
    ///
    /// Before #215, `AgentTTSComposer` wrote produced episodes into the Swift
    /// render store only. The kernel projection is now the source of truth and
    /// `applyKernelState` does a full-replace of `state.episodes`, so those
    /// legacy episodes would vanish on the first projection tick after a user
    /// updates to a #215+ build. This walks the still-persisted Swift state
    /// (captured before that first tick — see the call site in `attachKernel`),
    /// seeds the kernel's default "Agent Generated" podcast row, and registers
    /// each surviving episode so it rides the next projection back into the UI
    /// and becomes resolvable by `publish_episode`.
    ///
    /// Scope is intentionally narrow: only the default Agent Generated show,
    /// identified by its stable `sentinelFeedURL`. Other `.synthetic` shows are
    /// agent-OWNED podcasts whose kernel rows are seeded through their own
    /// create/publish lifecycle; a blind re-register there would orphan
    /// episodes under a missing row.
    ///
    /// The legacy show is matched by `sentinelFeedURL`, NOT by
    /// `defaultPodcastID`: that stable id was introduced in PR #215, but the
    /// pre-#215 `ensurePodcastID` created the synthetic `Podcast` row without an
    /// explicit id, so the initializer defaulted it to a random `UUID()`.
    /// Pre-#215 episodes are therefore parented to that random id. We resolve
    /// the legacy id(s) from the still-persisted `state.podcasts`, collect their
    /// episodes, and re-register them under the stable `defaultPodcastID` — the
    /// kernel row seeded just below — consolidating the show under one identity.
    func backfillSyntheticEpisodes() {
        let defaults = UserDefaults.standard
        guard !defaults.bool(forKey: Self.syntheticBackfillDoneKey) else { return }

        let defaultPodcastID = AgentGeneratedPodcastService.defaultPodcastID
        // Resolve the legacy (random-id) Agent Generated row(s) by the stable
        // sentinel feed URL, plus the stable id itself for episodes already
        // produced by a #215+ build that ran before this backfill shipped.
        let sentinel = AgentGeneratedPodcastService.sentinelFeedURL
        let legacyPodcastIDs = Set(
            state.podcasts
                .filter { $0.feedURL == sentinel || $0.id == defaultPodcastID }
                .map(\.id)
        )
        let legacyEpisodes = episodes.filter { legacyPodcastIDs.contains($0.podcastID) }
        guard !legacyEpisodes.isEmpty else {
            // Nothing to migrate (fresh install, or every episode already
            // produced by a #215+ build). Mark done so we never walk again.
            defaults.set(true, forKey: Self.syntheticBackfillDoneKey)
            return
        }

        // Seed the kernel's default synthetic podcast row first. Idempotent:
        // `create_synthetic_podcast` keys on the stable id, and the Swift
        // `upsertPodcast` mirror is insert-only by id. Without the row the
        // kernel would have nowhere to attach the registered episodes.
        _ = AgentGeneratedPodcastService.ensurePodcastID(in: self)

        var registered = 0
        for episode in legacyEpisodes {
            // Resolve the on-disk audio path: prefer the downloaded local file,
            // fall back to a `file://` enclosure URL. A synthetic episode is
            // produced as a downloaded m4a, so one of these is normally set.
            let downloadedPath: String? = {
                if case let .downloaded(localFileURL, _) = episode.downloadState {
                    return localFileURL.path
                }
                return nil
            }()
            let audioPath = downloadedPath
                ?? (episode.enclosureURL.isFileURL ? episode.enclosureURL.path : nil)
            guard let path = audioPath,
                  FileManager.default.fileExists(atPath: path) else {
                // Audio file is gone — registering would resurrect a dead row
                // that can never play. Skip it.
                continue
            }

            let chapterWire = (episode.chapters ?? []).map(AgentTTSComposer.chapterWire)
            let transcript = TranscriptStore.shared.load(episodeID: episode.id)?
                .segments.map(\.text).joined(separator: " ").nilIfEmpty

            kernelRegisterSyntheticEpisode(
                podcastId: defaultPodcastID.uuidString,
                episodeId: episode.id.uuidString,
                title: episode.title,
                audioPath: path,
                durationSecs: episode.duration,
                chapters: chapterWire,
                transcript: transcript
            )
            registered += 1
        }

        Self.backfillLogger.info(
            "Synthetic-episode backfill: registered \(registered, privacy: .public) of \(legacyEpisodes.count, privacy: .public) legacy agent episode(s) into the kernel"
        )
        // Flag set ONLY after the loop completes, so an interrupted pass retries.
        defaults.set(true, forKey: Self.syntheticBackfillDoneKey)
    }
}
