import SwiftUI

// MARK: - Evidence grade

/// The three editorial bands the wiki applies to a paragraph based on how
/// well-corroborated its underlying claim is. Mirrors UX-04 §7's
/// "low-evidence claims" treatment: green for multi-source consensus, amber
/// dotted for single-source, red for uncorroborated.
///
/// The grade is *editorial* — distinct from `WikiConfidenceBand`, which
/// scores the synthesizer's confidence in the *citation alignment*. A claim
/// can have high citation-alignment confidence and still earn a
/// `singleSource` grade because only one episode mentions it.
enum WikiEvidenceGrade: String, Hashable, Sendable {

    /// Multi-source: >=2 distinct episodes. Solid green left rule.
    case multiSource

    /// Single-source: exactly one episode. Amber, dotted left rule.
    case singleSource

    /// Uncorroborated: zero citations or general-knowledge sentence. Red
    /// dotted left rule.
    case uncorroborated

    /// VoiceOver label paired with the colored rule so the grade is
    /// announced rather than carried by color alone.
    var accessibilityLabel: String {
        switch self {
        case .multiSource: "multi-source evidence"
        case .singleSource: "single source"
        case .uncorroborated: "uncorroborated"
        }
    }
}

// MARK: - View modifier

/// Adds a 2-pixel left rule beside a paragraph, colored and styled
/// according to the supplied evidence grade. Designed to wrap any
/// claim-rendering view inside `WikiPageView`.
///
/// The modifier reads as a margin annotation: the rule sits flush to the
/// leading edge, the wrapped content keeps its own padding. Dotted strokes
/// are used for low-confidence grades so the treatment carries in
/// monochrome / Reduce Transparency modes.
struct EvidenceGradedRule: ViewModifier {

    let grade: WikiEvidenceGrade

    func body(content: Content) -> some View {
        HStack(alignment: .top, spacing: 12) {
            rule
            content
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityValue(grade.accessibilityLabel)
    }

    @ViewBuilder
    private var rule: some View {
        switch grade {
        case .multiSource:
            Rectangle()
                .fill(EvidenceGradedRule.multiSourceColor)
                .frame(width: 2)
                .frame(maxHeight: .infinity)
        case .singleSource:
            Rectangle()
                .fill(EvidenceGradedRule.singleSourceColor)
                .frame(width: 2)
                .frame(maxHeight: .infinity)
                .mask(dottedMask)
        case .uncorroborated:
            Rectangle()
                .fill(EvidenceGradedRule.uncorroboratedColor)
                .frame(width: 2)
                .frame(maxHeight: .infinity)
                .mask(dottedMask)
        }
    }

    private var dottedMask: some View {
        // Vertical dotted pattern realised by stroking a path with a dashed
        // line — gives the same effect as a `.dotted` stroke style on a
        // horizontal line, only oriented vertically.
        GeometryReader { geo in
            Path { path in
                path.move(to: CGPoint(x: geo.size.width / 2, y: 0))
                path.addLine(to: CGPoint(x: geo.size.width / 2, y: geo.size.height))
            }
            .stroke(style: StrokeStyle(
                lineWidth: geo.size.width,
                dash: [2, 3]
            ))
        }
    }

    // MARK: - Palette

    static let multiSourceColor = AppTheme.Tint.evidenceHigh
    static let singleSourceColor = AppTheme.Tint.evidenceMedium
    static let uncorroboratedColor = AppTheme.Tint.evidenceLow
}

// MARK: - Convenience

extension View {

    /// Sugar so call sites can write `claimView.evidenceGraded(.singleSource)`
    /// rather than spelling out the modifier.
    func evidenceGraded(_ grade: WikiEvidenceGrade) -> some View {
        modifier(EvidenceGradedRule(grade: grade))
    }
}

// MARK: - Bridging from WikiClaim

extension WikiClaim {

    /// Maps a `WikiClaim` onto the editorial evidence grade by counting
    /// distinct cited episodes. The mapping is intentionally simple —
    /// synthesizer confidence is left to the existing `WikiConfidenceBand`
    /// rule.
    var evidenceGrade: WikiEvidenceGrade {
        if isGeneralKnowledge && citations.isEmpty { return .uncorroborated }
        let distinctEpisodes = Set(citations.map(\.episodeID))
        switch distinctEpisodes.count {
        case 0: return .uncorroborated
        case 1: return .singleSource
        default: return .multiSource
        }
    }
}
