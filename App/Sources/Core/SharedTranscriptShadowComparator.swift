import Foundation
import Pod0Core

enum TranscriptShadowMismatch: String, CaseIterable, Sendable, Hashable {
    case identity
    case content
    case count
    case speaker
    case word
    case timestamp
    case provenance
}

enum SharedTranscriptShadowComparator {
    static func compare(
        authoritative: Transcript,
        podcastID: UUID,
        context: TranscriptObservationContext,
        summary: TranscriptSummaryProjection,
        candidate: Transcript
    ) -> Set<TranscriptShadowMismatch> {
        var mismatches: Set<TranscriptShadowMismatch> = []
        if summary.episodeId.uuid != authoritative.episodeID
            || candidate.episodeID != authoritative.episodeID
            || summary.podcastId.uuid != podcastID
            || summary.sourceRevision != context.sourceRevision
            || summary.sourcePayloadDigest.stableString.lowercased()
                != context.sourcePayloadDigest.lowercased() {
            mismatches.insert(.identity)
        }
        if candidate.language != authoritative.language
            || candidate.source != authoritative.source {
            mismatches.insert(.provenance)
        }
        let expectedWordCount = authoritative.segments.reduce(0) {
            $0 + UInt64($1.words?.count ?? 0)
        }
        if summary.speakerCount != UInt32(clamping: authoritative.speakers.count)
            || summary.segmentCount != UInt32(clamping: authoritative.segments.count)
            || summary.wordCount != expectedWordCount
            || candidate.speakers.count != authoritative.speakers.count
            || candidate.segments.count != authoritative.segments.count {
            mismatches.insert(.count)
        }
        compareSpeakers(authoritative, candidate, into: &mismatches)
        compareSegments(authoritative, candidate, into: &mismatches)
        if milliseconds(authoritative.generatedAt.timeIntervalSince1970)
            != milliseconds(candidate.generatedAt.timeIntervalSince1970) {
            mismatches.insert(.timestamp)
        }
        return mismatches
    }

    private static func compareSpeakers(
        _ authoritative: Transcript,
        _ candidate: Transcript,
        into mismatches: inout Set<TranscriptShadowMismatch>
    ) {
        for (left, right) in zip(authoritative.speakers, candidate.speakers) {
            if left.id != right.id
                || left.label != right.label
                || left.displayName != right.displayName {
                mismatches.insert(.speaker)
            }
        }
    }

    private static func compareSegments(
        _ authoritative: Transcript,
        _ candidate: Transcript,
        into mismatches: inout Set<TranscriptShadowMismatch>
    ) {
        for (left, right) in zip(authoritative.segments, candidate.segments) {
            if left.text != right.text { mismatches.insert(.content) }
            if left.speakerID != right.speakerID { mismatches.insert(.speaker) }
            if milliseconds(left.start) != milliseconds(right.start)
                || milliseconds(left.end) != milliseconds(right.end) {
                mismatches.insert(.timestamp)
            }
            compareWords(left.words ?? [], right.words ?? [], into: &mismatches)
        }
    }

    private static func compareWords(
        _ authoritative: [Word],
        _ candidate: [Word],
        into mismatches: inout Set<TranscriptShadowMismatch>
    ) {
        if authoritative.count != candidate.count { mismatches.insert(.word) }
        for (left, right) in zip(authoritative, candidate) {
            if left.text != right.text { mismatches.insert(.word) }
            if milliseconds(left.start) != milliseconds(right.start)
                || milliseconds(left.end) != milliseconds(right.end) {
                mismatches.insert(.timestamp)
            }
        }
    }

    private static func milliseconds(_ seconds: TimeInterval) -> UInt64? {
        try? TranscriptObservationMapper.milliseconds(seconds)
    }
}
