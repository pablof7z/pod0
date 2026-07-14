import Foundation

// MARK: - ChaptersClient

/// Fetches and decodes a Podcasting 2.0 chapters JSON file referenced by
/// `Episode.chaptersURL`. The format spec lives at
/// https://github.com/Podcastindex-org/podcast-namespace/blob/main/chapters/jsonChapters.md
///
/// Pure I/O. No `AppStateStore` writes — the caller (`ChaptersHydrationService`)
/// owns persistence so this stays unit-testable with a stubbable `URLSession`.
struct ChaptersClient: Sendable {

    enum FetchError: Error, Sendable, Equatable {
        case transport(String)
        case http(status: Int)
        case decode(String)
    }

    let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    /// Fetches the chapters JSON at `url` and returns the parsed chapters.
    /// Empty arrays are returned as an empty list (not an error) so callers
    /// can persist "we fetched, none exist" via the same path as success.
    func fetch(url: URL) async throws -> [Episode.Chapter] {
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        // Chapter JSONs are small (a few KB); 20s is plenty for a
        // healthy response and bails fast on hung hosts so the player
        // chrome doesn't sit waiting on the default 60s timeout.
        request.timeoutInterval = 20
        request.setValue("application/json, */*;q=0.8", forHTTPHeaderField: "Accept")
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw FetchError.transport(error.localizedDescription)
        }

        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            throw FetchError.http(status: http.statusCode)
        }

        return try Self.decode(data)
    }

    /// Shared decoder. `decode` runs once per `chaptersURL` per session
    /// (deduped by `ChaptersHydrationService`), so the per-call allocator
    /// pressure is modest — but every `static let`-decoder elsewhere in
    /// the codebase has followed the same pattern, so mirror it here for
    /// consistency.
    nonisolated(unsafe) private static let decoder = JSONDecoder()

    /// Decode a Podcasting 2.0 chapters JSON payload into `Episode.Chapter`
    /// values. Permissive: accepts integer or floating-point timestamps,
    /// missing optional fields, and skips entries with no title (the spec
    /// requires `title`, but real-world feeds occasionally publish
    /// title-less ad markers).
    static func decode(_ data: Data) throws -> [Episode.Chapter] {
        let envelope: ChaptersEnvelope
        do {
            envelope = try decoder.decode(ChaptersEnvelope.self, from: data)
        } catch {
            throw FetchError.decode(error.localizedDescription)
        }
        return envelope.chapters.compactMap { raw -> Episode.Chapter? in
            let trimmedTitle = raw.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !trimmedTitle.isEmpty else { return nil }
            return Episode.Chapter(
                startTime: raw.startTime ?? 0,
                endTime: raw.endTime,
                title: trimmedTitle,
                imageURL: raw.img.flatMap(URL.init(string:)),
                linkURL: raw.url.flatMap(URL.init(string:)),
                includeInTableOfContents: raw.toc ?? true
            )
        }
        .sorted { $0.startTime < $1.startTime }
    }

    // MARK: - Wire format

    private struct ChaptersEnvelope: Decodable {
        let chapters: [RawChapter]
    }

    private struct RawChapter: Decodable {
        let startTime: TimeInterval?
        let endTime: TimeInterval?
        let title: String?
        let img: String?
        let url: String?
        let toc: Bool?
    }
}
