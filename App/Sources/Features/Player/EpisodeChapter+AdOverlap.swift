import Foundation

extension Episode.Chapter {
    /// Pure presentation helper for the amber overlap stripe. Rust owns the
    /// authoritative spans; native computes only whether this visible chapter
    /// window should be decorated.
    func overlapsAd(
        in chapters: [Episode.Chapter],
        adSegments: [Episode.AdSegment]
    ) -> Bool {
        guard !adSegments.isEmpty else { return false }
        let effectiveEnd: TimeInterval
        if let endTime {
            effectiveEnd = endTime
        } else if let index = chapters.firstIndex(where: { $0.id == id }),
                  chapters.index(after: index) < chapters.endIndex {
            effectiveEnd = chapters[chapters.index(after: index)].startTime
        } else {
            effectiveEnd = .greatestFiniteMagnitude
        }
        return adSegments.contains { span in
            span.start < effectiveEnd && span.end > startTime
        }
    }
}
