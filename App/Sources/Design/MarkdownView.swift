import SwiftUI

/// Block-level markdown renderer for agent chat messages.
///
/// Handles: ATX headers (# / ## / ###), fenced code blocks (```),
/// bullet lists (- / *), numbered lists, GFM task lists (- [ ] / - [x]),
/// blockquotes (>), GFM tables, and paragraph text. Inline **bold**,
/// *italic*, and `code` are handled via AttributedString inline parsing.
///
/// SwiftUI's built-in Text(.init(markdown:)) collapses everything past
/// the first paragraph and ignores headers — this view works around that
/// by splitting into block elements and rendering each independently.
struct MarkdownView: View {

    private enum Layout {
        static let blockquoteBarWidth: CGFloat = 3
    }

    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                renderBlock(block)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var blocks: [Block] { Block.parse(text) }

    @ViewBuilder
    private func renderBlock(_ block: Block) -> some View {
        switch block {
        case .h1(let s):
            Text(inline(s))
                .font(AppTheme.Typography.title)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .h2(let s):
            Text(inline(s))
                .font(AppTheme.Typography.headline)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .h3(let s):
            Text(inline(s))
                .font(AppTheme.Typography.callout.weight(.semibold))
                .frame(maxWidth: .infinity, alignment: .leading)
        case .paragraph(let s):
            Text(inline(s))
                .font(AppTheme.Typography.body)
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .taskList(let items):
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.sm) {
                        Image(systemName: item.checked ? "checkmark.circle.fill" : "circle")
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(item.checked ? AnyShapeStyle(AppTheme.Tint.success) : AnyShapeStyle(Color.secondary))
                            .accessibilityHidden(true)
                        Text(inline(item.text))
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(item.checked ? AnyShapeStyle(Color.secondary) : AnyShapeStyle(Color.primary))
                            .strikethrough(item.checked, color: .secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        case .bullets(let items):
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.sm) {
                        Text("•")
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(.secondary)
                        Text(inline(item))
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(.primary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        case .numberedList(let items):
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.sm) {
                        Text("\(index + 1).")
                            .font(AppTheme.Typography.body.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .frame(minWidth: 20, alignment: .trailing)
                        Text(inline(item))
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(.primary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        case .quote(let s):
            HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
                Rectangle()
                    .fill(Color.secondary.opacity(0.4))
                    .frame(width: Layout.blockquoteBarWidth)
                Text(inline(s))
                    .font(AppTheme.Typography.body.italic())
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .fixedSize(horizontal: false, vertical: true)
        case .codeBlock(let language, let code):
            CodeBlockView(language: language, code: code)
        case .table(let headers, let rows):
            MarkdownTableView(headers: headers, rows: rows)
        }
    }

    private func inline(_ raw: String) -> AttributedString {
        let opts = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace
        )
        var attributed = (try? AttributedString(markdown: raw, options: opts)) ?? AttributedString(raw)
        Self.linkifyBareURLs(in: &attributed, source: raw)
        return attributed
    }

    // MARK: - URL autolinking

    /// Cached NSDataDetector for URL detection — thread-safe after construction.
    private static let urlDetector: NSDataDetector? = {
        try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)
    }()

    /// Finds bare URLs in `source` (the original Markdown string) and applies a
    /// `.link` attribute to the corresponding ranges in `attributed` — but only
    /// where no link attribute already exists (i.e. skipping `[text](url)` spans
    /// that AttributedString already wired up).
    private static func linkifyBareURLs(in attributed: inout AttributedString, source: String) {
        guard let detector = urlDetector else { return }
        let nsSource = source as NSString
        let fullRange = NSRange(location: 0, length: nsSource.length)
        let matches = detector.matches(in: source, options: [], range: fullRange)
        for match in matches {
            guard let url = match.url,
                  let swiftRange = Range(match.range, in: source) else { continue }
            // Map the Swift.String range to an AttributedString range.
            // AttributedString preserves character identity with the source when
            // constructed via inlineOnlyPreservingWhitespace, so offsets align.
            guard let attrRange = attributedRange(for: swiftRange, in: attributed) else { continue }
            // Skip spans that already carry a link (from `[text](url)` syntax).
            let existing = attributed[attrRange]
            if existing.link != nil { continue }
            attributed[attrRange].link = url
        }
    }

    /// Converts a `String` range to an `AttributedString` range by counting
    /// UTF-16 code units to locate the correct `AttributedString.Index` values.
    private static func attributedRange(
        for range: Range<String.Index>,
        in attributed: AttributedString
    ) -> Range<AttributedString.Index>? {
        let str = String(attributed.characters)
        guard range.lowerBound >= str.startIndex,
              range.upperBound <= str.endIndex else { return nil }
        let lower = AttributedString.Index(range.lowerBound, within: attributed)
        let upper = AttributedString.Index(range.upperBound, within: attributed)
        guard let lo = lower, let hi = upper, lo < hi else { return nil }
        return lo..<hi
    }

    // The `Block` parser lives in `MarkdownView+Block.swift`.
}
