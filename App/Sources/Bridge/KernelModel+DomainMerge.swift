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
        var social:    UInt64 = 0
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
        KernelModel.mergeDomainFramesImpl(frames, into: &composite, tracker: &tracker)
    }

    /// Pure static implementation — no `KernelModel` instance required.
    /// Called by the instance method above and directly by `@testable` unit
    /// tests (via `KernelDomainMergeTests`) so both paths exercise the same
    /// logic without duplication.
    @MainActor
    @discardableResult
    static func mergeDomainFramesImpl(
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
                composite.inboxLastTriagedAt = lib.inboxLastTriagedAt
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

        // ── social ───────────────────────────────────────────────────────────
        // Kernel-authoritative: when the social domain arrives, it REPLACES
        // the current nostrConversations slice. The kernel owns the NIP-10
        // thread projection; the Swift side only writes to this slice via
        // this merge path (recordNostrTurn is now LEGACY / local-only fallback).
        if let soc = frames.social {
            let lastRev = tracker.social
            if soc.rev > lastRev {
                tracker.social = soc.rev
                anyAccepted = true
                // social snapshot: tombstone semantics (nil = logged-out, clear).
                composite.social = soc.social
                // nostrConversations: kernel projection → wire composite (DTO slice).
                // The DTO→domain mapping happens downstream in
                // `projectSnapshotDerivedState` where the wire type is translated
                // into `[NostrConversationRecord]` and written to `AppState`.
                if let dtos = soc.nostrConversations {
                    composite.nostrConversations = dtos
                }
                dmLog.debug("social accepted rev=\(soc.rev)")
            } else {
                dmLog.debug("social DROPPED stale rev=\(soc.rev) last=\(lastRev)")
            }
        }

        // ── misc ─────────────────────────────────────────────────────────────
        // NOTE: social has moved to podcast.social (above).
        //       MiscDomainFrame no longer carries those fields.
        if let m = frames.misc {
            let lastRev = tracker.misc
            if m.rev > lastRev {
                tracker.misc = m.rev
                anyAccepted = true
                if let v = m.wikiArticles           { composite.wikiArticles = v }
                if let v = m.wikiSearchResults      { composite.wikiSearchResults = v }
                if let v = m.picks                  { composite.picks = v }
                if let v = m.agentTasks             { composite.agentTasks = v }
                if let v = m.knowledgeSearchResults { composite.knowledgeSearchResults = v }
                if let v = m.memoryFacts            { composite.memoryFacts = v }
                if let v = m.clips                  { composite.clips = v }
                if let v = m.comments               { composite.comments = v }
                if let v = m.feedbackEvents         { composite.feedbackEvents = v }
                if let v = m.feedbackThreads        { composite.feedbackThreads = v }
                // Tombstone semantics: nil = domain is now empty → clear slice.
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

// ─── DTO → Domain mapping ─────────────────────────────────────────────────────

extension KernelModel {
    /// Maps a wire `NostrConversationDTO` (snake_case → camelCase, Int timestamps,
    /// String direction) to the Swift domain type `NostrConversationRecord`
    /// (uppercase ID suffix, `Date` timestamps, `Direction` enum).
    ///
    /// CONTRACT:
    ///   - `dto.rootEventId`      → `record.rootEventID`   (lowercase d in DTO, uppercase ID in domain)
    ///   - `dto.counterpartyHex`  → `record.counterpartyPubkey`
    ///   - `dto.firstSeen`        → `record.firstSeen`     (Int unix → Date)
    ///   - `dto.lastActivity`     → `record.lastTouched`   (Int unix → Date)
    ///   - turn `"inbound"`       → `.incoming`
    ///   - turn `"outbound"`      → `.outgoing`
    ///   - `rawEventJSON` is not carried in the kernel projection → nil
    static func nostrConversationFromDTO(_ dto: NostrConversationDTO) -> NostrConversationRecord {
        let turns = dto.turns.map { t -> NostrConversationTurn in
            let dir: NostrConversationTurn.Direction = t.direction == "outbound" ? .outgoing : .incoming
            return NostrConversationTurn(
                eventID:      t.eventId,
                direction:    dir,
                pubkey:       t.pubkeyHex,
                createdAt:    Date(timeIntervalSince1970: Double(t.createdAt)),
                content:      t.content,
                rawEventJSON: nil
            )
        }
        return NostrConversationRecord(
            rootEventID:        dto.rootEventId,
            counterpartyPubkey: dto.counterpartyHex,
            firstSeen:          Date(timeIntervalSince1970: Double(dto.firstSeen)),
            lastTouched:        Date(timeIntervalSince1970: Double(dto.lastActivity)),
            turns:              turns
        )
    }
}
