import Foundation

// MARK: - DownloadsManagerProjection
//
// Rust owns download-row section membership and ordering. Swift resolves ids
// and renders native controls/details.

struct DownloadsManagerProjection {
    let activeEpisodeIDs: [UUID]
    let failedEpisodeIDs: [UUID]
    let downloadedEpisodeIDs: [UUID]

    var activeCount: Int { activeEpisodeIDs.count }
    var failedCount: Int { failedEpisodeIDs.count }
    var downloadedCount: Int { downloadedEpisodeIDs.count }

    static func load(store: AppStateStore) -> DownloadsManagerProjection {
        guard let envelope = store.kernel?.libraryDownloadRowsEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.downloadsManagerProjection.decode(Response.self, from: data)
        else {
            return DownloadsManagerProjection(
                activeEpisodeIDs: [],
                failedEpisodeIDs: [],
                downloadedEpisodeIDs: []
            )
        }
        return DownloadsManagerProjection(
            activeEpisodeIDs: decoded.activeEpisodeIds,
            failedEpisodeIDs: decoded.failedEpisodeIds,
            downloadedEpisodeIDs: decoded.downloadedEpisodeIds
        )
    }

    private struct Response: Decodable {
        let activeEpisodeIds: [UUID]
        let failedEpisodeIds: [UUID]
        let downloadedEpisodeIds: [UUID]
    }
}

private extension JSONDecoder {
    static let downloadsManagerProjection: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
