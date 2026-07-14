import Foundation
import os.log

// MARK: - PublisherTranscriptIngestor

/// Given an episode ID and a publisher-supplied transcript URL + MIME, picks
/// the right parser and produces a `Transcript`.
///
/// Maps:
///   - `application/json` (and Podcasting 2.0 JSON variants) → JSON parser
///   - `text/vtt`                                              → VTTParser
///   - `application/x-subrip`, `application/srt`, `text/srt`   → SRTParser
///   - extension fallbacks: `.json`, `.vtt`, `.srt`
///
/// Anything else (HTML, plain text) returns `Error.unsupported` — those need
/// the cloud transcription path.
struct PublisherTranscriptIngestor: Sendable {

    enum Error: Swift.Error, Sendable {
        case unsupported(mime: String?, url: URL)
        case fetchFailed(URL, underlying: String)
        case parseFailed(URL, underlying: String)
    }

    /// Pluggable fetch — overridable for tests so we don't hit the network.
    let fetch: @Sendable (URL) async throws -> (Data, String?)

    init(fetch: @escaping @Sendable (URL) async throws -> (Data, String?) = Self.defaultFetch) {
        self.fetch = fetch
    }

    /// Default fetch: `URLSession.shared`, returns body + response MIME.
    ///
    /// Goes through `URLRequest` so we can tighten the timeout and
    /// identify ourselves consistently with `FeedClient` and the AI
    /// catalog services. Without these:
    /// - A hung publisher transcript URL froze the loading state for
    ///   the full 60s default timeout before falling through to the
    ///   Scribe path.
    /// - Some publisher CDNs serve `.vtt` / `.srt` as `text/plain` to
    ///   default user agents, then send the correct MIME when an
    ///   `Accept` header advertises the formats we can parse.
    static let defaultFetch: @Sendable (URL) async throws -> (Data, String?) = { url in
        var request = URLRequest(url: url)
        request.timeoutInterval = 30
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")
        request.setValue(
            "application/json;q=1.0, text/vtt;q=0.9, application/x-subrip;q=0.9, */*;q=0.5",
            forHTTPHeaderField: "Accept"
        )
        let (data, response) = try await URLSession.shared.data(for: request)
        return (data, (response as? HTTPURLResponse)?.mimeType)
    }

    /// Ingests a transcript at `url` for `episodeID`. `mimeHint` comes from the
    /// `<podcast:transcript type="...">` attribute when known; we also fall
    /// back to the response MIME and to the path extension.
    func ingest(
        url: URL,
        mimeHint: String? = nil,
        episodeID: UUID,
        language: String = "en-US"
    ) async throws -> Transcript {
        let logger = Logger.app("PublisherTranscriptIngestor")
        logger.debug("Fetching publisher transcript: \(url.absoluteString, privacy: .public)")

        let (data, responseMime): (Data, String?)
        do {
            (data, responseMime) = try await fetch(url)
        } catch {
            throw Error.fetchFailed(url, underlying: String(describing: error))
        }

        let kind = TranscriptFormat.detect(
            mimeHint: mimeHint,
            responseMime: responseMime,
            url: url,
            sample: data
        )

        switch kind {
        case .podcastingJSON:
            do {
                return try PodcastingTranscriptJSONParser.parse(data, episodeID: episodeID, language: language)
            } catch {
                throw Error.parseFailed(url, underlying: String(describing: error))
            }
        case .vtt:
            guard let text = String(data: data, encoding: .utf8) else {
                throw Error.parseFailed(url, underlying: "VTT body was not UTF-8")
            }
            do {
                return try VTTParser.parse(text, episodeID: episodeID, language: language)
            } catch {
                throw Error.parseFailed(url, underlying: String(describing: error))
            }
        case .srt:
            guard let text = String(data: data, encoding: .utf8) else {
                throw Error.parseFailed(url, underlying: "SRT body was not UTF-8")
            }
            do {
                return try SRTParser.parse(text, episodeID: episodeID, language: language)
            } catch {
                throw Error.parseFailed(url, underlying: String(describing: error))
            }
        case .unsupported:
            throw Error.unsupported(mime: mimeHint ?? responseMime, url: url)
        }
    }
}

// MARK: - TranscriptFormat detection

enum TranscriptFormat: Sendable {
    case podcastingJSON
    case vtt
    case srt
    case unsupported

    /// Layered detection: explicit MIME hint wins, then response MIME, then
    /// path extension, then a sniff of the first ~256 bytes.
    static func detect(
        mimeHint: String?,
        responseMime: String?,
        url: URL,
        sample: Data
    ) -> TranscriptFormat {
        for candidate in [mimeHint, responseMime] {
            guard let raw = candidate?.lowercased() else { continue }
            if raw.contains("application/json") || raw.contains("application/srt+json") {
                return .podcastingJSON
            }
            if raw.contains("text/vtt") { return .vtt }
            if raw.contains("application/x-subrip") || raw.contains("application/srt") || raw.contains("text/srt") {
                return .srt
            }
        }
        switch url.pathExtension.lowercased() {
        case "json": return .podcastingJSON
        case "vtt": return .vtt
        case "srt": return .srt
        default: break
        }
        // Sniff the first non-whitespace character.
        if let head = String(data: sample.prefix(256), encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        {
            if head.hasPrefix("{") { return .podcastingJSON }
            if head.hasPrefix("WEBVTT") { return .vtt }
            // SRT starts with "1\n00:..." — a number line followed by timing.
            if head.first?.isNumber == true && head.contains("-->") { return .srt }
        }
        return .unsupported
    }
}
