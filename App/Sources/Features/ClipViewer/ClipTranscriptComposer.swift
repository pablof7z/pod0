import Foundation

// MARK: - ClipTranscriptComposer

/// Turns a `Transcript` plus the reader's clips into the row list the clip
/// reader renders.
///
/// The shape of the result is the design: the clip you opened sits at full
/// presence, a few segments either side recede, everything else folds — except
/// your *other* clips in this episode, which never fold. That produces the
/// rhythm mark / silence / mark, so scrolling the screen is scrolling your own
/// marks rather than the episode.
enum ClipTranscriptComposer {

    /// Segments of context kept either side of the clip the reader opened.
    static let focusContext = 3

    /// Segments of context kept either side of the reader's other clips —
    /// enough to seat them, not enough to compete with the focused passage.
    static let markContext = 1

    static func rows(
        transcript: Transcript,
        clips: [Clip],
        focusedClipID: UUID,
        annotatedClipIDs: Set<UUID>,
        expandedFolds: Set<String> = []
    ) -> [ClipTranscriptRow] {
        let segments = transcript.segments.sorted { $0.start < $1.start }
        guard !segments.isEmpty else { return [] }

        let names = speakerNames(in: transcript)
        let owners = clipOwnership(segments: segments, clips: clips, focusedClipID: focusedClipID)
        var visible = visibleIndices(count: segments.count, owners: owners, focusedClipID: focusedClipID)
        visible.formUnion(expandedIndices(count: segments.count, visible: visible, expanded: expandedFolds))

        return assemble(
            segments: segments,
            owners: owners,
            visible: visible,
            names: names,
            focusedClipID: focusedClipID,
            annotatedClipIDs: annotatedClipIDs
        )
    }

    /// Fold identifiers are keyed on the index the hidden run starts at, so an
    /// expanded fold reveals exactly the run the reader tapped and leaves every
    /// other fold alone.
    private static func expandedIndices(
        count: Int,
        visible: Set<Int>,
        expanded: Set<String>
    ) -> Set<Int> {
        guard !expanded.isEmpty else { return [] }
        var revealed: Set<Int> = []
        var index = 0
        while index < count {
            guard !visible.contains(index) else {
                index += 1
                continue
            }
            let start = index
            var run: [Int] = []
            while index < count, !visible.contains(index) {
                run.append(index)
                index += 1
            }
            if expanded.contains("fold-\(start)") {
                revealed.formUnion(run)
            }
        }
        return revealed
    }

    // MARK: - Ownership

    /// Maps each segment index to the clip covering it. When clips overlap the
    /// focused clip wins, so opening a clip always shows *that* clip at focus.
    private static func clipOwnership(
        segments: [Segment],
        clips: [Clip],
        focusedClipID: UUID
    ) -> [Int: UUID] {
        var owners: [Int: UUID] = [:]
        for clip in clips where !clip.deleted {
            for (index, segment) in segments.enumerated() where overlaps(segment, clip) {
                if owners[index] == nil || clip.id == focusedClipID {
                    owners[index] = clip.id
                }
            }
        }
        return owners
    }

    /// A segment belongs to a clip when the two spans overlap at all — clip
    /// boundaries are sentence-snapped, but a segment can still straddle one.
    private static func overlaps(_ segment: Segment, _ clip: Clip) -> Bool {
        segment.end > clip.startSeconds && segment.start < clip.endSeconds
    }

    // MARK: - Visibility

    private static func visibleIndices(
        count: Int,
        owners: [Int: UUID],
        focusedClipID: UUID
    ) -> Set<Int> {
        var visible: Set<Int> = []
        for (index, clipID) in owners {
            visible.insert(index)
            let pad = clipID == focusedClipID ? focusContext : markContext
            let lower = max(0, index - pad)
            let upper = min(count - 1, index + pad)
            if lower <= upper {
                visible.formUnion(lower...upper)
            }
        }
        return visible
    }

    // MARK: - Assembly

    private static func assemble(
        segments: [Segment],
        owners: [Int: UUID],
        visible: Set<Int>,
        names: [UUID: String],
        focusedClipID: UUID,
        annotatedClipIDs: Set<UUID>
    ) -> [ClipTranscriptRow] {
        var rows: [ClipTranscriptRow] = []
        var index = 0
        var previousSpeaker: UUID??

        while index < segments.count {
            if !visible.contains(index) {
                let start = index
                var run = 0
                while index < segments.count, !visible.contains(index) {
                    run += 1
                    index += 1
                }
                rows.append(.fold(id: "fold-\(start)", weight: foldWeight(forRun: run)))
                // A fold resets the speaker run — the name reappears after it.
                previousSpeaker = nil
                continue
            }

            let merged = mergeTurn(segments: segments, owners: owners, visible: visible, from: &index)
            let clipID = merged.clipID
            let presence: ClipTranscriptTurn.Presence = {
                guard let clipID else { return .context }
                return clipID == focusedClipID ? .focus : .mark
            }()

            let showsName = previousSpeaker != .some(merged.speakerID)
            previousSpeaker = .some(merged.speakerID)

            rows.append(.turn(ClipTranscriptTurn(
                id: merged.id,
                presence: presence,
                speakerName: showsName ? merged.speakerID.flatMap { names[$0] } : nil,
                text: merged.text,
                start: merged.start,
                end: merged.end,
                clipID: clipID,
                isAnnotated: clipID.map { annotatedClipIDs.contains($0) } ?? false
            )))
        }

        return rows
    }

    private struct MergedTurn {
        let id: String
        let speakerID: UUID?
        let clipID: UUID?
        let text: String
        let start: TimeInterval
        let end: TimeInterval
    }

    /// Merges the run of adjacent segments sharing a speaker *and* a clip
    /// membership. Both must match: a turn that straddled a clip boundary
    /// could not be rendered at one weight of presence.
    private static func mergeTurn(
        segments: [Segment],
        owners: [Int: UUID],
        visible: Set<Int>,
        from index: inout Int
    ) -> MergedTurn {
        let first = segments[index]
        let speakerID = first.speakerID
        let clipID = owners[index]
        let id = "turn-\(index)-\(first.id.uuidString)"
        var parts = [first.text]
        var end = first.end
        index += 1

        while index < segments.count,
              visible.contains(index),
              segments[index].speakerID == speakerID,
              owners[index] == clipID {
            parts.append(segments[index].text)
            end = segments[index].end
            index += 1
        }

        return MergedTurn(
            id: id,
            speakerID: speakerID,
            clipID: clipID,
            text: parts.joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines),
            start: first.start,
            end: end
        )
    }

    /// How many silhouette lines a fold draws. Bounded so a forty-minute gap
    /// and a two-minute gap both stay a pause rather than becoming a feature.
    private static func foldWeight(forRun run: Int) -> Int {
        min(4, max(2, run / 3 + 2))
    }

    // MARK: - Speakers

    private static func speakerNames(in transcript: Transcript) -> [UUID: String] {
        var names: [UUID: String] = [:]
        for speaker in transcript.speakers {
            let resolved = speaker.displayName ?? speaker.label
            let trimmed = resolved.trimmingCharacters(in: .whitespacesAndNewlines)
            names[speaker.id] = trimmed.isEmpty ? speaker.label : trimmed
        }
        return names
    }
}
