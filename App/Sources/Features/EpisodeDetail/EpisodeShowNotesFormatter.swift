import Foundation

/// Helpers for rendering an `Episode.description` value, which may be HTML,
/// plain text, or a mix. We do the cheap thing: strip tags for the body text
/// surface, and decode the most common HTML entities so apostrophes and dashes
/// don't show up as `&amp;rsquo;` to the reader.
///
/// We deliberately avoid `NSAttributedString(data:options:.html…)` here:
/// it requires a main-thread WebKit pass that's both expensive and prone to
/// crashing under SwiftUI preview snapshots. A future pass can swap in a
/// proper attributed renderer once the in-app HTML→AttributedString utility
/// in `Design/MarkdownView.swift` is generalized.
enum EpisodeShowNotesFormatter {

    /// Plain-text projection of an episode description. Tag stripping +
    /// entity decoding + whitespace normalization, plus a fix-up pass that
    /// removes the spurious space `stripTags` injects before trailing
    /// punctuation (`<b>word</b>.` → `word .` → `word.`).
    static func plainText(from raw: String) -> String {
        let stripped = stripTags(raw)
        let decoded = decodeEntities(stripped)
        let collapsed = collapseWhitespace(decoded)
        return collapsed.replacingOccurrences(
            of: "\\s+([.,!?;:])",
            with: "$1",
            options: .regularExpression
        )
    }

    private static func stripTags(_ input: String) -> String {
        // Replace each tag with a single space so block-level boundaries
        // (`</p><p>`, `<br>`) don't glue adjacent words together —
        // `<p>A</p><p>B</p>` previously collapsed to `AB`. The trailing
        // `collapseWhitespace` pass folds multiple spaces back to one.
        var inTag = false
        var out = ""
        out.reserveCapacity(input.count)
        for c in input {
            if c == "<" {
                inTag = true
                if out.last != " " { out.append(" ") }
                continue
            }
            if c == ">" { inTag = false; continue }
            if !inTag { out.append(c) }
        }
        return out
    }

    private static let entityMap: [String: String] = [
        "&amp;": "&",
        "&lt;": "<",
        "&gt;": ">",
        "&quot;": "\"",
        "&apos;": "'",
        "&nbsp;": " ",
        "&rsquo;": "\u{2019}",
        "&lsquo;": "\u{2018}",
        "&rdquo;": "\u{201D}",
        "&ldquo;": "\u{201C}",
        "&hellip;": "\u{2026}",
        "&mdash;": "\u{2014}",
        "&ndash;": "\u{2013}"
    ]

    private static func decodeEntities(_ input: String) -> String {
        var out = input
        for (entity, replacement) in entityMap {
            out = out.replacingOccurrences(of: entity, with: replacement)
        }
        return decodeNumericEntities(out)
    }

    /// Decodes decimal (`&#39;`) and hexadecimal (`&#x2019;`) numeric
    /// character references. WordPress-generated feeds emit these
    /// constantly for smart quotes / apostrophes / dashes; the
    /// previous formatter only handled named entities and let the
    /// numeric escapes bleed through verbatim.
    private static func decodeNumericEntities(_ input: String) -> String {
        // Cheap pre-check — if there's no "&#" anywhere, the whole
        // walk is wasted work.
        guard input.contains("&#") else { return input }

        var out = ""
        out.reserveCapacity(input.count)
        var i = input.startIndex
        while i < input.endIndex {
            // Need at least "&#x;" worth of room left to be a valid
            // reference, so bail to literal copy when it's shorter.
            if input[i] == "&",
               input.index(i, offsetBy: 3, limitedBy: input.endIndex) != nil,
               input[input.index(after: i)] == "#",
               // Cap the lookahead — a ref this long is malformed
               // and a runaway scan over a description body would be
               // its own problem.
               let semi = input[i...].prefix(12).firstIndex(of: ";")
            {
                let body = input[input.index(i, offsetBy: 2)..<semi]
                if let scalar = parseNumericRef(body) {
                    out.append(Character(scalar))
                    i = input.index(after: semi)
                    continue
                }
            }
            out.append(input[i])
            i = input.index(after: i)
        }
        return out
    }

    /// Parses the digits between `&#` and `;`. Decimal by default,
    /// hex when the body starts with `x` or `X`. Returns `nil` for
    /// malformed input or out-of-range / surrogate scalars so the
    /// caller falls through to a literal-character copy.
    private static func parseNumericRef(_ body: Substring) -> Unicode.Scalar? {
        guard !body.isEmpty else { return nil }
        let value: UInt32?
        if let first = body.first, first == "x" || first == "X" {
            value = UInt32(body.dropFirst(), radix: 16)
        } else {
            value = UInt32(body, radix: 10)
        }
        guard let v = value else { return nil }
        return Unicode.Scalar(v)
    }

    private static func collapseWhitespace(_ input: String) -> String {
        let lines = input
            .split(whereSeparator: \.isNewline)
            .map { line -> String in
                // Fold repeated spaces / tabs / etc. inside the line into a
                // single space so the de-tagged stream (which inserts a
                // space at every tag boundary) doesn't read with awkward
                // multi-space gaps.
                line
                    .replacingOccurrences(
                        of: "[ \\t]+",
                        with: " ",
                        options: .regularExpression
                    )
                    .trimmingCharacters(in: .whitespaces)
            }
            .filter { !$0.isEmpty }
        return lines.joined(separator: "\n\n")
    }
}
