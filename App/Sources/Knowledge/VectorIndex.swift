import Foundation
import SQLiteVec

enum VectorStoreError: LocalizedError {
    case dimensionMismatch(expected: Int, got: Int)
    case backingStorageFailure(String)

    var errorDescription: String? {
        switch self {
        case .dimensionMismatch(let expected, let got):
            "Embedding dimension mismatch: expected \(expected), got \(got)."
        case .backingStorageFailure(let detail):
            "Recall capability storage failure: \(detail)"
        }
    }
}

/// Reconstructible native vector capability for the Rust recall kernel.
/// It stores no authoritative evidence identity, ranking policy, or selected
/// generation; every row is rebuilt from a bounded Rust evidence projection.
actor VectorIndex {
    static let embeddingDimensions = 1_024

    let db: Database
    let dimensions: Int
    let embedder: any EmbeddingsClient
    var recallSchemaReady = false

    init(
        embedder: any EmbeddingsClient,
        fileURL: URL? = nil,
        inMemory: Bool = false,
        dimensions: Int = VectorIndex.embeddingDimensions
    ) throws {
        try SQLiteVec.initialize()
        self.dimensions = dimensions
        self.embedder = embedder
        if inMemory {
            db = try Database(.inMemory)
        } else {
            db = try Database(.uri(try (fileURL ?? Self.defaultStoreURL()).path))
        }
    }

    static func defaultStoreURL() throws -> URL {
        let fileManager = FileManager.default
        let support = try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = support.appendingPathComponent("podcastr", isDirectory: true)
        if !fileManager.fileExists(atPath: directory.path) {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        return directory.appendingPathComponent("vectors.sqlite")
    }

    static func sanitizeFTSQuery(_ raw: String) -> String {
        raw.unicodeScalars
            .map { CharacterSet.alphanumerics.contains($0) ? String($0) : " " }
            .joined()
            .split(whereSeparator: \.isWhitespace)
            .joined(separator: " ")
    }
}
