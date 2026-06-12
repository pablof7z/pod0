import AuthenticationServices
import Foundation
import os.log
import UIKit

@MainActor
final class BYOKConnectService: NSObject, ASWebAuthenticationPresentationContextProviding {
    private let logger = Logger.app("BYOKConnectService")
    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    private let redirectScheme = "podcastr"
    private let redirectHost = "byok"
    private var currentSession: ASWebAuthenticationSession?

    func connectPodcastProviders() async throws -> [BYOKProviderToken] {
        try await connectProviders(BYOKProvider.podcastPlayerDefaults)
    }

    func connectProviders(_ providers: [BYOKProvider]) async throws -> [BYOKProviderToken] {
        var seenProviders = Set<String>()
        let uniqueProviders = providers.filter { seenProviders.insert($0.rawValue).inserted }
        let pending = try makeAuthorization(providers: uniqueProviders)
        let callbackURL = try await authenticate(url: pending.authorizationURL)
        let response = try await exchangeAuthorization(callbackURL: callbackURL, pending: pending)
        let requested = Set(uniqueProviders.map(\.rawValue))
        let returned = response.providers.filter { requested.contains($0.provider) && !$0.apiKey.isEmpty }
        guard !returned.isEmpty else {
            throw BYOKConnectError.noProviderKeysReturned
        }
        return returned
    }

    func connectOpenRouter() async throws -> BYOKTokenResponse {
        try await connectProvider(.openRouter)
    }

    func connectElevenLabs() async throws -> BYOKTokenResponse {
        try await connectProvider(.elevenLabs)
    }

    func connectAssemblyAI() async throws -> BYOKTokenResponse {
        try await connectProvider(.assemblyAI)
    }

    func connectOllama() async throws -> BYOKTokenResponse {
        try await connectProvider(.ollama)
    }

    func connectPerplexity() async throws -> BYOKTokenResponse {
        try await connectProvider(.perplexity)
    }

    private func connectProvider(_ provider: BYOKProvider) async throws -> BYOKTokenResponse {
        let pending = try makeAuthorization(providers: [provider])
        let callbackURL = try await authenticate(url: pending.authorizationURL)
        let token = try await exchangeAuthorization(callbackURL: callbackURL, pending: pending)
        guard token.provider == provider.rawValue else {
            throw BYOKConnectError.unexpectedProvider
        }
        guard token.tokenType == "raw_api_key", !token.apiKey.isEmpty else {
            throw BYOKConnectError.invalidTokenResponse
        }
        return token
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        if let activeScene = scenes.first(where: { $0.activationState == .foregroundActive }),
           let keyWindow = activeScene.windows.first(where: { $0.isKeyWindow }) {
            return keyWindow
        }
        if let fallbackWindow = scenes.flatMap(\.windows).first {
            return fallbackWindow
        }
        if let firstScene = scenes.first {
            return UIWindow(windowScene: firstScene)
        }
        preconditionFailure("BYOKConnectService: no UIWindowScene available to present authentication")
    }

    private func makeAuthorization(providers: [BYOKProvider]) throws -> BYOKPendingAuthorization {
        guard !providers.isEmpty else {
            throw BYOKConnectError.noProviderKeysReturned
        }
        let redirectURI = "\(redirectScheme)://\(redirectHost)"
        let intent = BYOKAuthorizationIntent(
            providers: providers.map(\.rawValue),
            redirectURI: redirectURI,
            clientID: clientID,
            appName: appName
        )
        let intentJSON = try Self.encoder.encode(intent)
        guard let intentString = String(data: intentJSON, encoding: .utf8) else {
            throw BYOKConnectError.invalidAuthorizationURL
        }
        let responseJSON = intentString.withCString { intentPtr in
            guard let ptr = nmp_app_podcast_byok_authorization(intentPtr) else {
                return #"{"error":{"kind":"invalid_authorization_url","message":"null response from Rust"}}"#
            }
            defer { nmp_free_string(ptr) }
            return String(cString: ptr)
        }
        let envelope = try decodeEnvelope(
            responseJSON,
            as: BYOKPendingAuthorization.self,
            fallback: .invalidAuthorizationURL
        )
        return envelope
    }

    private func authenticate(url: URL) async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            let session = ASWebAuthenticationSession(url: url, callbackURLScheme: redirectScheme) { [weak self] callbackURL, error in
                Task { @MainActor in
                    self?.currentSession = nil

                    if let error {
                        if let authError = error as? ASWebAuthenticationSessionError,
                           authError.code == .canceledLogin {
                            continuation.resume(throwing: BYOKConnectError.cancelled)
                            return
                        }
                        continuation.resume(throwing: BYOKConnectError.authenticationFailed)
                        return
                    }

                    guard let callbackURL else {
                        continuation.resume(throwing: BYOKConnectError.invalidCallback)
                        return
                    }
                    continuation.resume(returning: callbackURL)
                }
            }

            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = false
            currentSession = session

            guard session.start() else {
                currentSession = nil
                continuation.resume(throwing: BYOKConnectError.authenticationFailed)
                return
            }
        }
    }

    private func exchangeAuthorization(
        callbackURL: URL,
        pending: BYOKPendingAuthorization
    ) async throws -> BYOKTokenResponse {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw BYOKConnectError.tokenExchangeFailed
        }
        let intent = BYOKExchangeIntent(pending: pending, callbackURL: callbackURL.absoluteString)
        let intentJSON = try Self.encoder.encode(intent)
        guard let intentString = String(data: intentJSON, encoding: .utf8) else {
            throw BYOKConnectError.invalidTokenResponse
        }
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"token_exchange_failed","message":"Kernel handle unavailable"}}"#
            }
            return intentString.withCString { intentPtr in
                guard let ptr = nmp_app_podcast_byok_exchange(handle, intentPtr) else {
                    return #"{"error":{"kind":"token_exchange_failed","message":"null response from Rust"}}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value
        return try decodeEnvelope(
            responseJSON,
            as: BYOKTokenResponse.self,
            fallback: .invalidTokenResponse
        )
    }

    private var clientID: String {
        Bundle.main.bundleIdentifier ?? "com.podcastr.podcastr"
    }

    private var appName: String {
        if let displayName = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String,
           !displayName.isEmpty {
            return displayName
        }
        return Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Podcastr"
    }

    private func decodeEnvelope<T: Decodable>(
        _ responseJSON: String,
        as type: T.Type,
        fallback: BYOKConnectError
    ) throws -> T {
        guard let responseData = responseJSON.data(using: .utf8) else {
            throw fallback
        }
        do {
            let envelope = try Self.decoder.decode(BYOKBackendEnvelope<T>.self, from: responseData)
            if let error = envelope.error {
                throw byokError(from: error, fallback: fallback)
            }
            guard let result = envelope.result else {
                throw fallback
            }
            return result
        } catch let error as BYOKConnectError {
            throw error
        } catch {
            logger.error("BYOK backend response decode failed: \(error, privacy: .public)")
            throw fallback
        }
    }

    private func byokError(
        from error: BYOKBackendError,
        fallback: BYOKConnectError
    ) -> BYOKConnectError {
        switch error.kind {
        case "access_denied":
            return .accessDenied
        case "invalid_authorization_url":
            return .invalidAuthorizationURL
        case "invalid_callback":
            return .invalidCallback
        case "invalid_token_response":
            return .invalidTokenResponse
        case "missing_code":
            return .missingCode
        case "no_provider_keys_returned", "empty_providers":
            return .noProviderKeysReturned
        case "random_generation_failed":
            return .randomGenerationFailed
        case "server_rejected_token":
            return .serverRejectedToken(error: error.message)
        case "state_mismatch":
            return .stateMismatch
        case "token_exchange_failed":
            return .tokenExchangeFailed
        case "unexpected_provider":
            return .unexpectedProvider
        default:
            return fallback
        }
    }
}

private struct BYOKAuthorizationIntent: Encodable {
    let providers: [String]
    let redirectURI: String
    let clientID: String
    let appName: String

    private enum CodingKeys: String, CodingKey {
        case providers
        case redirectURI = "redirect_uri"
        case clientID = "client_id"
        case appName = "app_name"
    }
}

private struct BYOKExchangeIntent: Encodable {
    let pending: BYOKPendingAuthorization
    let callbackURL: String

    private enum CodingKeys: String, CodingKey {
        case pending
        case callbackURL = "callback_url"
    }
}

private struct BYOKBackendEnvelope<Result: Decodable>: Decodable {
    let result: Result?
    let error: BYOKBackendError?
}

private struct BYOKBackendError: Decodable {
    let kind: String
    let message: String?
}
