import Foundation
import CoreTransferable
import UniformTypeIdentifiers

// MARK: - OPMLExportFile

/// Transferable OPML 2.0 document built from the live `model.library` snapshot.
/// Used by `OPMLTab`'s `ShareLink` to surface a system share sheet that emits
/// a real `.opml` file (so other podcast apps recognize it on the receiving
/// side) rather than dumping XML text into a Message.
///
/// All emission lives here so the view stays presentational and the OPML
/// shape stays in one place. Output format mirrors `podcast-feeds::export_opml`
/// (which is the Rust-side equivalent — same outline schema, same escaping
/// rules) so a re-import via the Rust kernel round-trips losslessly.
struct OPMLExportFile: Transferable {
    let data: Data
    let suggestedFilename: String

    static var transferRepresentation: some TransferRepresentation {
        DataRepresentation(exportedContentType: .xml) { file in
            file.data
        }
        .suggestedFileName { $0.suggestedFilename }
    }

    /// Build an OPML document from the current podcast library snapshot.
    /// Entries without a `feedUrl` are skipped (synthetic shows that can't be
    /// re-imported — there's nothing to round-trip).
    static func from(library: [PodcastSummary]) -> OPMLExportFile {
        let xml = render(library: library, dateCreated: Date())
        let data = xml.data(using: .utf8) ?? Data()
        return OPMLExportFile(data: data, suggestedFilename: "Subscriptions.opml")
    }

    /// Render the OPML XML body. Split out for testability — the date and
    /// library list are both inputs so the output is deterministic.
    static func render(library: [PodcastSummary], dateCreated: Date) -> String {
        var lines: [String] = []
        lines.append(#"<?xml version="1.0" encoding="UTF-8"?>"#)
        lines.append(#"<opml version="2.0">"#)
        lines.append("  <head>")
        lines.append("    <title>\(escape("Podcastr Subscriptions"))</title>")
        lines.append("    <dateCreated>\(rfc822(dateCreated))</dateCreated>")
        lines.append("  </head>")
        lines.append("  <body>")
        lines.append(#"    <outline text="feeds" title="feeds">"#)

        for podcast in library {
            guard let feedURL = podcast.feedUrl, !feedURL.isEmpty else { continue }
            let attrs: [(String, String)] = [
                ("type", "rss"),
                ("text", podcast.title),
                ("title", podcast.title),
                ("xmlUrl", feedURL),
            ]
            let rendered = attrs
                .map { (k, v) in "\(k)=\"\(escape(v))\"" }
                .joined(separator: " ")
            lines.append("      <outline \(rendered) />")
        }

        lines.append("    </outline>")
        lines.append("  </body>")
        lines.append("</opml>")
        return lines.joined(separator: "\n")
    }

    // MARK: - Private helpers

    private static func escape(_ s: String) -> String {
        var out = ""
        out.reserveCapacity(s.count)
        for ch in s {
            switch ch {
            case "&": out.append("&amp;")
            case "<": out.append("&lt;")
            case ">": out.append("&gt;")
            case "\"": out.append("&quot;")
            case "'": out.append("&apos;")
            case "\n", "\r": out.append(" ")
            default: out.append(ch)
            }
        }
        return out
    }

    private static func rfc822(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "GMT")
        formatter.dateFormat = "EEE, dd MMM yyyy HH:mm:ss 'GMT'"
        return formatter.string(from: date)
    }
}
