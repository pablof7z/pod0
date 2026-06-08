import CryptoKit
import Foundation

// MARK: - BlossomUploader
//
// Blossom BUD-02 upload over HTTPS, for the two IN-MEMORY upload callers whose
// blob is signed by the user's ACTIVE account: avatar (`ChangePhotoSheet`) and
// owned-podcast artwork (`LiveAgentOwnedPodcastManager.generateAndUploadArtwork`).
// Hashes the payload, signs a kind:24242 authorization event THROUGH THE KERNEL
// (D13 — no signing in Swift), PUTs `/upload`, and returns the descriptor URL
// the server hands back. One Blossom host — no abstraction layer, no fallback
// list. If the default goes down, swap `defaultServer`. Only WHO signs moved
// from the pre-D13 implementation (Swift `LocalKeySigner` → kernel `KernelSigner`);
// the kind:24242 auth tags/semantics are unchanged.
//
// WHY this HTTP transport stays in Swift (it is NOT a lazy deferral):
//
// The AUDIO upload path is already fully Rust-owned — `host_op_publish::
// publish_episode` calls `blossom::upload_to_blossom`, which reads the local
// download file and signs the kind:24242 auth event SYNCHRONOUSLY with the
// per-podcast NIP-F4 key (raw `secret_bytes` it already holds). Rust can own a
// Blossom upload precisely when it can sign synchronously with a local key.
//
// These two callers cannot meet that bar. They sign with the user's ACTIVE
// account, which may be a NIP-46 bunker — and active-account signing only goes
// through the ASYNC sign-and-return seam (`nmp_app_sign_event_for_return`:
// correlation-id + host-side continuation, deliberately host-driven because a
// remote-signer round-trip cannot block the kernel actor thread). A synchronous
// Rust host-op handler cannot orchestrate that suspend/resume, so routing these
// uploads through Rust would require a whole new async-bridging capability
// (register a `signed_events` observer, oneshot-bridge the signed event, then
// dispatch HTTP, then stamp a projection). That capability is the genuine
// blocker — NOT a missing `file_path` (the bytes-vs-path framing was a red
// herring; writing the in-memory blob to a temp file would still not unblock
// the active-account signing).
//
// Tracked in docs/BACKLOG.md → `blossom-active-account-upload-kernel`. Until
// that capability lands, signing already goes through the kernel here (D13) and
// only the HTTP transport remains in Swift, kept as one coherent path so the
// kind:24242 auth-event construction policy is not fragmented across the
// boundary (AGENTS.md anti-fragmentation).

protocol BlossomUploading: Sendable {
    /// Upload `data` to the Blossom server. Returns the absolute URL the
    /// server stored the blob at. `signer` produces the kind:24242 auth event;
    /// pass a `KernelSigner` so the kernel signs (D13), never Swift.
    func upload(data: Data, contentType: String, signer: any NostrSigner) async throws -> URL
}

struct BlossomUploader: BlossomUploading {

    /// Default Blossom server (blossom.primal.net per project config).
    static let defaultServer = URL(string: "https://blossom.primal.net")!

    let server: URL
    let session: URLSession

    init(server: URL = BlossomUploader.defaultServer, session: URLSession = .shared) {
        self.server = server
        self.session = session
    }

    /// Convenience init that accepts a raw URL string from `Settings.blossomServerURL`.
    /// Falls back to `defaultServer` when the string is empty or malformed.
    init(serverURLString: String, session: URLSession = .shared) {
        let parsed = URL(string: serverURLString.trimmed)
        self.init(server: parsed ?? BlossomUploader.defaultServer, session: session)
    }

    func upload(data: Data, contentType: String, signer: any NostrSigner) async throws -> URL {
        let hashHex = Data(SHA256.hash(data: data)).hexString
        let now = Int(Date().timeIntervalSince1970)
        let description: String
        switch contentType {
        case "audio/mpeg", "audio/mp4", "audio/m4a": description = "Upload podcast audio"
        case "application/json":                      description = "Upload podcast data"
        case "text/vtt", "text/plain":                description = "Upload transcript"
        case "image/jpeg", "image/png", "image/webp": description = "Upload podcast artwork"
        default:                                      description = "Upload file"
        }
        let draft = NostrEventDraft(
            kind: 24242,
            content: description,
            tags: [
                ["t", "upload"],
                ["x", hashHex],
                ["expiration", String(now + 60 * 5)],
            ],
            createdAt: now
        )
        let signed = try await signer.sign(draft)
        let authJSON = try JSONSerialization.data(withJSONObject: eventDictionary(signed), options: [])
        let authB64 = authJSON.base64EncodedString()

        var request = URLRequest(url: server.appendingPathComponent("upload"))
        request.httpMethod = "PUT"
        request.setValue("Nostr \(authB64)", forHTTPHeaderField: "Authorization")
        request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        request.setValue(String(data.count), forHTTPHeaderField: "Content-Length")
        request.httpBody = data

        let (responseData, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw BlossomUploadError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            // Blossom servers convey rejection details in the `X-Reason` header
            // per BUD-01 §4. Fall back to the body if absent.
            let reason = http.value(forHTTPHeaderField: "X-Reason")
                ?? String(data: responseData, encoding: .utf8).flatMap { $0.isEmpty ? nil : $0 }
                ?? "HTTP \(http.statusCode)"
            throw BlossomUploadError.serverRejected(reason)
        }
        guard let object = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let urlString = object["url"] as? String,
              let url = URL(string: urlString) else {
            throw BlossomUploadError.malformedDescriptor
        }
        return url
    }

    private func eventDictionary(_ event: SignedNostrEvent) -> [String: Any] {
        [
            "id": event.id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.sig,
        ]
    }
}

enum BlossomUploadError: LocalizedError {
    case invalidResponse
    case serverRejected(String)
    case malformedDescriptor

    var errorDescription: String? {
        switch self {
        case .invalidResponse: return "Upload server did not respond."
        case .serverRejected(let reason): return "Upload rejected: \(reason)"
        case .malformedDescriptor: return "Upload server returned a malformed response."
        }
    }
}
