import Foundation

// MARK: - BlossomUploadError

enum BlossomUploadError: LocalizedError {
    case serverRejected(String)
    case malformedDescriptor

    var errorDescription: String? {
        switch self {
        case .serverRejected(let reason): return "Upload rejected: \(reason)"
        case .malformedDescriptor: return "Upload server returned a malformed response."
        }
    }
}

// MARK: - KernelModel + Blossom kernel upload (nmp.blossom.upload, D13/D0)
//
// The kernel owns the full Build → Sign → Transport pipeline for BUD-02 blob
// uploads (nmp-blossom v0.6.0). Swift writes the blob to a temp file, dispatches
// `nmp.blossom.upload` with a correlation-id, and awaits the `BlobDescriptor`
// from the drain-once `action_results` typed sidecar via `actionResultsRegistry`.
//
// No signing in Swift; no HTTP in Swift (D13 — kernel signs; D0 — kernel
// transports). Reactive — the result arrives as a push frame update.
//
// File split from KernelModel.swift to keep both files under the AGENTS.md
// 500-line soft limit.

extension KernelModel {

    /// Dispatch `nmp.blossom.upload` and await the `BlobDescriptor.url` from
    /// the kernel's `action_results` drain.
    ///
    /// - Parameters:
    ///   - data: Raw blob bytes (image / audio / etc.). Written to a temp file
    ///     before dispatch so the kernel can stream from disk (D8).
    ///   - contentType: MIME type (e.g. `"image/jpeg"`, `"image/png"`).
    ///   - servers: Blossom server base URLs (e.g. `["https://blossom.primal.net"]`).
    ///   - signerPubkeyHex: Optional roster key hex for per-podcast NIP-F4
    ///     signing. `nil` / empty → active account (avatar and artwork callers).
    /// - Returns: The permanent blob URL the server stored the upload at.
    func blossomUpload(
        data: Data,
        contentType: String,
        servers: [String],
        signerPubkeyHex: String? = nil
    ) async throws -> URL {
        // Write the blob to a temp file; the kernel streams it from disk (D8).
        let ext: String
        switch contentType {
        case "image/jpeg":                            ext = "jpg"
        case "image/png":                             ext = "png"
        case "image/webp":                            ext = "webp"
        case "audio/mpeg", "audio/mp4", "audio/m4a": ext = "m4a"
        default:                                      ext = "bin"
        }
        let tmpURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension(ext)
        try data.write(to: tmpURL)
        defer { try? FileManager.default.removeItem(at: tmpURL) }

        var body: [String: Any] = [
            "file_path": tmpURL.path,
            "content_type": contentType,
            "servers": servers,
        ]
        if let signer = signerPubkeyHex, !signer.isEmpty {
            body["signer_pubkey"] = signer
        }
        // Use `dispatchSilent` so errors surface as thrown (not as a toast).
        let result = dispatchSilent(namespace: "nmp.blossom.upload", body: body)
        guard case let .accepted(correlationId: correlationID) = result, !correlationID.isEmpty else {
            if case let .failure(msg) = result {
                throw BlossomUploadError.serverRejected(msg)
            }
            throw BlossomUploadError.serverRejected("dispatch returned empty correlation id")
        }

        // Await the BlobDescriptor from the drain-once action_results projection.
        // Race against a 60-second caller-owned deadline (a bunker round-trip
        // can be slow, but uploads are bounded).
        let registry = actionResultsRegistry
        let entry = try await withThrowingTaskGroup(of: ActionResultEntry.self) { group in
            group.addTask {
                return try await registry.awaitResult(correlationID: correlationID)
            }
            group.addTask {
                try? await Task.sleep(for: .seconds(60))
                registry.cancel(
                    correlationID: correlationID,
                    with: BlossomUploadError.serverRejected("upload timed out"))
                try await Task.sleep(for: .seconds(3600))
                throw BlossomUploadError.serverRejected("upload timed out")
            }
            defer { group.cancelAll() }
            guard let first = try await group.next() else {
                throw BlossomUploadError.serverRejected("upload timed out")
            }
            return first
        }

        // The `result` field carries the serialised BlobDescriptor JSON
        // (`{ "url": "…", "sha256": "…", "size": N, "uploaded": N }`).
        guard let resultJSON = entry.resultJSON,
              let resultData = resultJSON.data(using: String.Encoding.utf8),
              let obj = try? JSONSerialization.jsonObject(with: resultData) as? [String: Any],
              let urlString = obj["url"] as? String,
              let url = URL(string: urlString)
        else {
            if let err = entry.error, !err.isEmpty {
                throw BlossomUploadError.serverRejected(err)
            }
            throw BlossomUploadError.malformedDescriptor
        }
        return url
    }
}
