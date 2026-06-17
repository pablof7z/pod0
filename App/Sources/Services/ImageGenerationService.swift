import Foundation

// MARK: - ImageGenerationService
//
// Generates images through shared Rust provider transport. Swift owns only the
// app-facing API and receives image bytes for platform-specific upload/storage.

protocol ImageGenerating: Sendable {
    func generate(prompt: String, model: String) async throws -> Data
}

struct ImageGenerationService: ImageGenerating {

    func generate(prompt: String, model: String) async throws -> Data {
        try Task.checkCancellation()
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else {
            throw ImageGenerationError.invalidResponse
        }
        let request: [String: Any] = [
            "prompt": prompt,
            "model": model
        ]
        let requestData = try JSONSerialization.data(withJSONObject: request)
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw ImageGenerationError.malformedResponse
        }

        let responseJSON: String = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"null kernel handle"}"#
            }
            return requestJSON.withCString { requestPtr in
                guard let ptr = nmp_app_podcast_generate_image(handle, requestPtr) else {
                    return #"{"error":"null response from Rust"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        try Task.checkCancellation()
        guard let responseData = responseJSON.data(using: .utf8),
              let response = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
            throw ImageGenerationError.malformedResponse
        }
        if let error = response["error"] as? String {
            throw ImageGenerationError.serverError(error)
        }
        if let b64 = response["image_base64"] as? String,
           let imageData = Data(base64Encoded: b64) {
            return imageData
        }
        throw ImageGenerationError.malformedResponse
    }
}

enum ImageGenerationError: LocalizedError {
    case invalidResponse
    case serverError(String)
    case malformedResponse

    var errorDescription: String? {
        switch self {
        case .invalidResponse: return "Image generation server did not respond."
        case .serverError(let msg): return "Image generation failed: \(msg)"
        case .malformedResponse: return "Image generation returned unexpected data."
        }
    }
}
