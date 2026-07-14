import Foundation
import os.log

// MARK: - ImageGenerationService
//
// Generates images via OpenRouter. Multimodal models (Gemini "Banana" variants,
// GPT-image variants) use /chat/completions where images come back in
// message.images alongside message.content. Legacy DALL-E / FLUX models use
// /images/generations. Routing is determined automatically from the model ID.

protocol ImageGenerating: Sendable {
    func generate(prompt: String, model: String) async throws -> Data
}

struct ImageGenerationService: ImageGenerating {

    private static let logger = Logger.app("ImageGenerationService")
    private static let chatEndpoint = URL(string: "https://openrouter.ai/api/v1/chat/completions")!
    private static let imagesEndpoint = URL(string: "https://openrouter.ai/api/v1/images/generations")!

    let apiKey: String
    let session: URLSession

    init(apiKey: String, session: URLSession = .shared) {
        self.apiKey = apiKey
        self.session = session
    }

    func generate(prompt: String, model: String) async throws -> Data {
        if Self.usesChatCompletions(model: model) {
            return try await generateViaChat(prompt: prompt, model: model)
        }
        return try await generateViaImages(prompt: prompt, model: model)
    }

    // MARK: - Routing

    // Multimodal image generators (text+image->text+image) route through /chat/completions.
    // Only DALL-E and FLUX-style models use /images/generations.
    private static func usesChatCompletions(model: String) -> Bool {
        let lc = model.lowercased()
        if lc.contains("dall-e") { return false }
        if lc.hasPrefix("black-forest-labs/") { return false }
        return true
    }

    // MARK: - Chat completions path

    private func generateViaChat(prompt: String, model: String) async throws -> Data {
        var request = URLRequest(url: Self.chatEndpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let body: [String: Any] = [
            "model": model,
            "messages": [["role": "user", "content": prompt]],
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (responseData, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw ImageGenerationError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            let reason = String(data: responseData, encoding: .utf8) ?? "HTTP \(http.statusCode)"
            Self.logger.error("Chat image generation failed (\(http.statusCode, privacy: .public)): \(reason, privacy: .public)")
            throw ImageGenerationError.serverError(reason)
        }

        return try await extractImageFromChatResponse(responseData)
    }

    private func extractImageFromChatResponse(_ data: Data) async throws -> Data {
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choices = json["choices"] as? [[String: Any]],
              let message = choices.first?["message"] as? [String: Any] else {
            throw ImageGenerationError.malformedResponse
        }

        // Per OpenRouter docs: image output lives in message.images alongside message.content
        if let images = message["images"] as? [[String: Any]],
           let first = images.first,
           let imageUrl = first["image_url"] as? [String: Any],
           let urlStr = imageUrl["url"] as? String {
            return try await resolveImageURL(urlStr)
        }

        // Fallback: some models embed images inside message.content as a parts array
        if let contentArray = message["content"] as? [[String: Any]] {
            for item in contentArray where item["type"] as? String == "image_url" {
                if let imageUrl = item["image_url"] as? [String: Any],
                   let urlStr = imageUrl["url"] as? String {
                    return try await resolveImageURL(urlStr)
                }
            }
        }

        throw ImageGenerationError.malformedResponse
    }

    // MARK: - Images endpoint path (legacy DALL-E / FLUX)

    private func generateViaImages(prompt: String, model: String) async throws -> Data {
        var request = URLRequest(url: Self.imagesEndpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let body: [String: Any] = [
            "model": model,
            "prompt": prompt,
            "n": 1,
            "size": "1024x1024",
            "response_format": "b64_json",
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (responseData, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw ImageGenerationError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            let reason = String(data: responseData, encoding: .utf8) ?? "HTTP \(http.statusCode)"
            Self.logger.error("Image generation failed (\(http.statusCode, privacy: .public)): \(reason, privacy: .public)")
            throw ImageGenerationError.serverError(reason)
        }

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let dataArray = json["data"] as? [[String: Any]],
              let first = dataArray.first else {
            throw ImageGenerationError.malformedResponse
        }

        if let b64 = first["b64_json"] as? String, let imageData = Data(base64Encoded: b64) {
            return imageData
        }
        if let urlString = first["url"] as? String, let url = URL(string: urlString) {
            let (imageData, _) = try await session.data(from: url)
            return imageData
        }
        throw ImageGenerationError.malformedResponse
    }

    // MARK: - Helpers

    private func resolveImageURL(_ urlStr: String) async throws -> Data {
        if urlStr.hasPrefix("data:") {
            let parts = urlStr.components(separatedBy: ",")
            guard parts.count >= 2, let imageData = Data(base64Encoded: parts[1]) else {
                throw ImageGenerationError.malformedResponse
            }
            return imageData
        }
        guard let url = URL(string: urlStr) else {
            throw ImageGenerationError.malformedResponse
        }
        let (imageData, _) = try await session.data(from: url)
        return imageData
    }
}

enum ImageGenerationError: LocalizedError {
    case noAPIKey
    case invalidResponse
    case serverError(String)
    case malformedResponse

    var errorDescription: String? {
        switch self {
        case .noAPIKey: return "No OpenRouter API key configured."
        case .invalidResponse: return "Image generation server did not respond."
        case .serverError(let msg): return "Image generation failed: \(msg)"
        case .malformedResponse: return "Image generation returned unexpected data."
        }
    }
}
