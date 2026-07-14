import SwiftUI

// MARK: - ClipComposerHandlesView

/// Two draggable handles bracketing the clip's `[start, end]` range. Drags
/// snap to sentence boundaries (the segment list passed in). The handle view
/// is purely presentational: it reports proposed millisecond-values upward
/// and the parent applies the snap.
///
/// We render a horizontal track of segment ticks rather than a waveform —
/// the audio asset isn't necessarily downloaded by the time the composer
/// opens, and the sentence-grid view aligns with the "snap to meaning"
/// mental model from UX-03 §5.
struct ClipComposerHandlesView: View {

    // MARK: Inputs

    let segments: [Segment]
    @Binding var startMs: Int
    @Binding var endMs: Int

    // MARK: Layout constants

    private let trackHeight: CGFloat = 56
    private let handleWidth: CGFloat = 12

    // MARK: Body

    var body: some View {
        GeometryReader { geo in
            let trackWidth = geo.size.width
            let bounds = totalBounds()
            let startX = position(forMs: startMs, in: trackWidth, bounds: bounds)
            let endX   = position(forMs: endMs, in: trackWidth, bounds: bounds)

            ZStack(alignment: .leading) {
                // Track background
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color.secondary.opacity(0.12))
                    .frame(height: trackHeight)

                // Sentence ticks
                HStack(spacing: 0) {
                    ForEach(segments) { _ in
                        Rectangle()
                            .fill(Color.secondary.opacity(0.20))
                            .frame(width: 0.5)
                            .frame(maxHeight: .infinity)
                        Spacer(minLength: 0)
                    }
                }
                .frame(height: trackHeight)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

                // Selection band
                Rectangle()
                    .fill(Color.accentColor.opacity(0.22))
                    .frame(width: max(2, endX - startX), height: trackHeight)
                    .offset(x: startX)

                // Start handle
                handleShape
                    .offset(x: startX - handleWidth / 2)
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .onChanged { value in
                                let proposed = msFromX(value.location.x, width: trackWidth, bounds: bounds)
                                let snapped  = snapped(toStartNear: proposed)
                                if snapped < endMs {
                                    if snapped != startMs { Haptics.selection() }
                                    startMs = snapped
                                }
                            }
                    )

                // End handle
                handleShape
                    .offset(x: endX - handleWidth / 2)
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .onChanged { value in
                                let proposed = msFromX(value.location.x, width: trackWidth, bounds: bounds)
                                let snapped  = snapped(toEndNear: proposed)
                                if snapped > startMs {
                                    if snapped != endMs { Haptics.selection() }
                                    endMs = snapped
                                }
                            }
                    )
            }
            .frame(height: trackHeight)
        }
        .frame(height: trackHeight)
    }

    // MARK: - Geometry

    private var handleShape: some View {
        RoundedRectangle(cornerRadius: 4, style: .continuous)
            .fill(Color.accentColor)
            .frame(width: handleWidth, height: trackHeight + 12)
            .shadow(color: .black.opacity(0.18), radius: 4, y: 2)
    }

    private func totalBounds() -> (lowerMs: Int, upperMs: Int) {
        guard let first = segments.first, let last = segments.last else {
            return (lowerMs: 0, upperMs: 1)
        }
        let lower = Int(first.start * 1000)
        let upper = max(Int(last.end * 1000), lower + 1)
        return (lower, upper)
    }

    private func position(forMs ms: Int, in width: CGFloat, bounds: (lowerMs: Int, upperMs: Int)) -> CGFloat {
        let span = max(1, bounds.upperMs - bounds.lowerMs)
        let frac = CGFloat(ms - bounds.lowerMs) / CGFloat(span)
        return min(max(frac, 0), 1) * width
    }

    private func msFromX(_ x: CGFloat, width: CGFloat, bounds: (lowerMs: Int, upperMs: Int)) -> Int {
        let span = bounds.upperMs - bounds.lowerMs
        let frac = max(0, min(1, x / max(1, width)))
        return bounds.lowerMs + Int(round(Double(span) * Double(frac)))
    }

    // MARK: - Snap

    /// Snap a proposed millisecond value to the nearest segment *start*. Used
    /// by the leading handle so the clip always begins on a sentence boundary.
    private func snapped(toStartNear ms: Int) -> Int {
        guard let nearest = segments.min(by: { lhs, rhs in
            abs(Int(lhs.start * 1000) - ms) < abs(Int(rhs.start * 1000) - ms)
        }) else { return ms }
        return Int(nearest.start * 1000)
    }

    /// Snap a proposed millisecond value to the nearest segment *end*. Used
    /// by the trailing handle so the clip always ends on a sentence boundary.
    private func snapped(toEndNear ms: Int) -> Int {
        guard let nearest = segments.min(by: { lhs, rhs in
            abs(Int(lhs.end * 1000) - ms) < abs(Int(rhs.end * 1000) - ms)
        }) else { return ms }
        return Int(nearest.end * 1000)
    }
}
