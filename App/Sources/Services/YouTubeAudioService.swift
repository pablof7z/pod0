import Foundation
import os.log

// MARK: - YouTubeAudioService
//
// Calls the user-configured YouTube audio extraction endpoint.
//
// Video fetch — POST <extractorURL>  body: {"url": "<youtube_url>"}
//   Response: {"audio_url": "…", "title": "…", "author": "…", "duration_seconds": 1234}
//
// Search — POST <extractorURL>  body: {"search": "<query>", "limit": N}
//   Response: {"results": [{"url": "…", "title": "…", "author": "…", "duration_seconds": 1234}, …]}
//
// Compatible with cobalt (https://cobalt.tools) and simple yt-dlp wrappers.

struct YouTubeVideoInfo: Sendable {
    let audioURL: URL
    let title: String
    let author: String
    let durationSeconds: TimeInterval?
}

struct YouTubeSearchResult: Sendable {
    let url: String
    let title: String
    let author: String
    let durationSeconds: TimeInterval?
}

enum YouTubeAudioServiceError: LocalizedError {
    case notConfigured
    case invalidExtractorURL(String)
    case invalidYouTubeURL(String)
    case requestFailed(Int, String)
    case noAudioURL
    case networkError(Error)

    var errorDescription: String? {
        switch self {
        case .notConfigured:
            return "No YouTube extractor endpoint configured. Add one in Settings → Providers → YouTube Ingestion."
        case .invalidExtractorURL(let url):
            return "The configured extractor URL is invalid: \(url)"
        case .invalidYouTubeURL(let url):
            return "Not a valid YouTube URL: \(url)"
        case .requestFailed(let status, let body):
            return "Extractor returned HTTP \(status): \(body.prefix(200))"
        case .noAudioURL:
            return "Extractor response did not include an audio URL."
        case .networkError(let err):
            return "Network error contacting extractor: \(err.localizedDescription)"
        }
    }
}

struct YouTubeAudioService: Sendable {

    private static let logger = Logger.app("YouTubeAudioService")

    /// Fetches audio info for a YouTube video URL using the configured extractor endpoint.
    func fetchVideoInfo(youtubeURL: String, extractorURLString: String) async throws -> YouTubeVideoInfo {
        guard let extractorURL = URL(string: extractorURLString) else {
            throw YouTubeAudioServiceError.invalidExtractorURL(extractorURLString)
        }
        guard isValidYouTubeURL(youtubeURL) else {
            throw YouTubeAudioServiceError.invalidYouTubeURL(youtubeURL)
        }

        var request = URLRequest(url: extractorURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let body = ["url": youtubeURL]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw YouTubeAudioServiceError.networkError(error)
        }

        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw YouTubeAudioServiceError.requestFailed(http.statusCode, body)
        }

        return try parseResponse(data: data)
    }

    // MARK: - Search

    func searchVideos(query: String, limit: Int, extractorURLString: String) async throws -> [YouTubeSearchResult] {
        guard let extractorURL = URL(string: extractorURLString) else {
            throw YouTubeAudioServiceError.invalidExtractorURL(extractorURLString)
        }

        var request = URLRequest(url: extractorURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.httpBody = try JSONSerialization.data(withJSONObject: ["search": query, "limit": limit])

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw YouTubeAudioServiceError.networkError(error)
        }

        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw YouTubeAudioServiceError.requestFailed(http.statusCode, body)
        }

        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let rawResults = json["results"] as? [[String: Any]] else {
            throw YouTubeAudioServiceError.noAudioURL
        }

        return rawResults.compactMap(parseSearchHit)
    }

    private func parseSearchHit(_ json: [String: Any]) -> YouTubeSearchResult? {
        guard let url = (json["url"] as? String), !url.isEmpty else { return nil }
        let title = (json["title"] as? String) ?? "YouTube Video"
        let author = (json["author"] as? String)
            ?? (json["channel"] as? String)
            ?? (json["uploader"] as? String)
            ?? "YouTube"
        let duration: TimeInterval? = {
            if let d = json["duration_seconds"] as? Double { return d }
            if let d = json["duration_seconds"] as? Int { return TimeInterval(d) }
            if let d = json["duration"] as? Double { return d }
            if let d = json["duration"] as? Int { return TimeInterval(d) }
            return nil
        }()
        return YouTubeSearchResult(url: url, title: title, author: author, durationSeconds: duration)
    }

    // MARK: - Parsing

    private func parseResponse(data: Data) throws -> YouTubeVideoInfo {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw YouTubeAudioServiceError.noAudioURL
        }

        // Accept either "audio_url" (generic) or "url" (cobalt-style).
        let rawURL = (json["audio_url"] as? String) ?? (json["url"] as? String) ?? ""
        guard !rawURL.isEmpty, let audioURL = URL(string: rawURL) else {
            throw YouTubeAudioServiceError.noAudioURL
        }

        let title = (json["title"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let author = (json["author"] as? String)
            ?? (json["channel"] as? String)
            ?? (json["uploader"] as? String)
            ?? ""
        let durationSeconds: TimeInterval? = {
            if let d = json["duration_seconds"] as? Double { return d }
            if let d = json["duration_seconds"] as? Int { return TimeInterval(d) }
            if let d = json["duration"] as? Double { return d }
            if let d = json["duration"] as? Int { return TimeInterval(d) }
            return nil
        }()

        return YouTubeVideoInfo(
            audioURL: audioURL,
            title: title.isEmpty ? "YouTube Video" : title,
            author: author.isEmpty ? "YouTube" : author,
            durationSeconds: durationSeconds
        )
    }

    // MARK: - Validation

    private func isValidYouTubeURL(_ urlString: String) -> Bool {
        guard let url = URL(string: urlString),
              let host = url.host?.lowercased() else { return false }
        return host == "youtube.com"
            || host == "www.youtube.com"
            || host == "m.youtube.com"
            || host == "youtu.be"
            || host == "music.youtube.com"
    }
}
