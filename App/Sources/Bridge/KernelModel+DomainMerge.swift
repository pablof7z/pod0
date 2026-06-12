import Foundation
import os.log

private let dmLog = Logger(subsystem: "io.f7z.podcast", category: "DomainMerge")

// ─── Per-domain rev tracking (drop-guard) ────────────────────────────────────
//
// Each domain frame carries a monotonically increasing `rev`. If a frame
// arrives with `rev <= lastApplied[domain]` it is stale (out-of-order,
// duplicate, or a burst that overtook an earlier decode) and MUST be dropped
// without touching the composite. The guard is per-domain so a fast-cycling
// playback domain cannot starve a slower-cycling library domain.

extension KernelModel {
    // ── Per-domain last-applied revs ─────────────────────────────────────────
    struct DomainRevTracker {
        var library:   UInt64 = 0
        var playback:  UInt64 = 0
        var downloads: UInt64 = 0
        var settings:  UInt64 = 0
        var identity:  UInt64 = 0
        var widget:    UInt64 = 0
        var misc:      UInt64 = 0
    }
}

// ─── Composite merge ─────────────────────────────────────────────────────────

extension KernelModel {
    /// Merge present domain frames from a push frame into `compositeUpdate`.
    ///
    /// For each domain present in `frames`:
    ///   1. Drop-guard: skip if `frame.rev <= lastApplied[domain]`.
    ///   2. Merge that domain's fields into the composite.
    ///   3. Advance `lastApplied[domain]`.
    ///
    /// Returns `true` when at least one domain was accepted (the composite
    /// changed and should flow through `applyPodcastUpdate`); `false` when
    /// all present domains were stale (whole frame is a no-op).
    @MainActor
    @discardableResult
    func mergeDomainFrames(
        _ frames: PodcastDomainFrames,
        into composite: inout PodcastUpdate,
        tracker: inout DomainRevTracker
    ) -> Bool {
        var anyAccepted = false

        // ── library ──────────────────────────────────────────────────────────
        if let lib = frames.library {
            let lastRev = tracker.library
            if lib.rev > lastRev {
                tracker.library = lib.rev
                anyAccepted = true
                composite.library          = lib.library ?? []
                composite.categories       = lib.categories ?? []
                composite.searchResults    = lib.searchResults ?? []
                composite.nostrResults     = lib.nostrResults ?? []
                composite.ownedPodcasts    = lib.ownedPodcasts ?? []
                composite.inbox            = lib.inbox ?? []
                composite.inboxTriageInProgress = lib.inboxTriageInProgress ?? false
                dmLog.debug("library accepted rev=\(lib.rev)")
            } else {
                dmLog.debug("library DROPPED stale rev=\(lib.rev) last=\(lastRev)")
            }
        }

        // ── playback ─────────────────────────────────────────────────────────
        if let play = frames.playback {
            let lastRev = tracker.playback
            if play.rev > lastRev {
                tracker.playback = play.rev
                anyAccepted = true
                composite.nowPlaying = play.nowPlaying
                composite.queue      = play.queue ?? []
                dmLog.debug("playback accepted rev=\(play.rev)")
            } else {
                dmLog.debug("playback DROPPED stale rev=\(play.rev) last=\(lastRev)")
            }
        }

        // ── downloads ────────────────────────────────────────────────────────
        if let dl = frames.downloads {
            let lastRev = tracker.downloads
            if dl.rev > lastRev {
                tracker.downloads = dl.rev
                anyAccepted = true
                composite.downloads = dl.downloads
                dmLog.debug("downloads accepted rev=\(dl.rev)")
            } else {
                dmLog.debug("downloads DROPPED stale rev=\(dl.rev) last=\(lastRev)")
            }
        }

        // ── settings ─────────────────────────────────────────────────────────
        if let sett = frames.settings {
            let lastRev = tracker.settings
            if sett.rev > lastRev {
                tracker.settings = sett.rev
                anyAccepted = true
                if let s = sett.settings { composite.settings = s }
                if let r = sett.configuredRelays { composite.configuredRelays = r }
                dmLog.debug("settings accepted rev=\(sett.rev)")
            } else {
                dmLog.debug("settings DROPPED stale rev=\(sett.rev) last=\(lastRev)")
            }
        }

        // ── identity ─────────────────────────────────────────────────────────
        if let ident = frames.identity {
            let lastRev = tracker.identity
            if ident.rev > lastRev {
                tracker.identity = ident.rev
                anyAccepted = true
                composite.activeAccount = ident.activeAccount
                dmLog.debug("identity accepted rev=\(ident.rev)")
            } else {
                dmLog.debug("identity DROPPED stale rev=\(ident.rev) last=\(lastRev)")
            }
        }

        // ── widget ───────────────────────────────────────────────────────────
        if let wid = frames.widget {
            let lastRev = tracker.widget
            if wid.rev > lastRev {
                tracker.widget = wid.rev
                anyAccepted = true
                composite.widget = wid.widget
                dmLog.debug("widget accepted rev=\(wid.rev)")
            } else {
                dmLog.debug("widget DROPPED stale rev=\(wid.rev) last=\(lastRev)")
            }
        }

        // ── misc ─────────────────────────────────────────────────────────────
        if let m = frames.misc {
            let lastRev = tracker.misc
            if m.rev > lastRev {
                tracker.misc = m.rev
                anyAccepted = true
                if let v = m.wikiArticles          { composite.wikiArticles = v }
                if let v = m.wikiSearchResults     { composite.wikiSearchResults = v }
                if let v = m.picks                 { composite.picks = v }
                if let v = m.agentTasks            { composite.agentTasks = v }
                if let v = m.knowledgeSearchResults { composite.knowledgeSearchResults = v }
                if let v = m.memoryFacts           { composite.memoryFacts = v }
                if let v = m.clips                 { composite.clips = v }
                if let v = m.agentNotes            { composite.agentNotes = v }
                if let v = m.comments              { composite.comments = v }
                if let v = m.feedbackEvents        { composite.feedbackEvents = v }
                if let v = m.feedbackThreads       { composite.feedbackThreads = v }
                // Tombstone semantics: nil = domain is now empty → clear slice.
                composite.social       = m.social
                composite.voice        = m.voice
                composite.agent        = m.agent
                composite.agentContext = m.agentContext
                dmLog.debug("misc accepted rev=\(m.rev)")
            } else {
                dmLog.debug("misc DROPPED stale rev=\(m.rev) last=\(lastRev)")
            }
        }

        return anyAccepted
    }
}
