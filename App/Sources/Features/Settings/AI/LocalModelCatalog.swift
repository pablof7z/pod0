import Foundation

struct LocalModelSpec: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let displayName: String
    let description: String
    let sizeBytes: Int64
    let downloadURL: URL
    let minDeviceRAMGB: Int

    enum CodingKeys: String, CodingKey {
        case id
        case displayName = "display_name"
        case description
        case sizeBytes = "size_bytes"
        case downloadURL = "download_url"
        case minDeviceRAMGB = "min_device_ram_gb"
    }

    init(
        id: String,
        displayName: String,
        description: String,
        sizeBytes: Int64,
        downloadURL: URL,
        minDeviceRAMGB: Int
    ) {
        self.id = id
        self.displayName = displayName
        self.description = description
        self.sizeBytes = sizeBytes
        self.downloadURL = downloadURL
        self.minDeviceRAMGB = minDeviceRAMGB
    }
}

enum LocalModelCatalog {
    static func fetch() async -> LocalModelCatalogLoad {
        await LocalModelCatalogService().fetchSpecs()
    }
}

enum LocalModelCatalogLoad: Sendable {
    case loaded([LocalModelSpec])
    case failed(LocalModelCatalogError)
}

enum LocalModelCatalogError: LocalizedError {
    case kernelUnavailable
    case invalidResponse
    case decoding(String)

    var errorDescription: String? {
        switch self {
        case .kernelUnavailable:
            return "App backend is not ready yet. Try again in a moment."
        case .invalidResponse:
            return "Unexpected local model catalog response."
        case .decoding(let message):
            return message
        }
    }
}

struct LocalModelCatalogService: Sendable {
    func fetchSpecs() async -> LocalModelCatalogLoad {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            return .failed(.kernelUnavailable)
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"Kernel handle unavailable"}"#
            }
            guard let ptr = nmp_app_podcast_local_model_catalog(handle) else {
                return #"{"error":"null response from Rust"}"#
            }
            defer { nmp_free_string(ptr) }
            return String(cString: ptr)
        }.value

        guard let data = responseJSON.data(using: .utf8) else {
            return .failed(.invalidResponse)
        }
        do {
            let envelope = try JSONDecoder().decode(LocalModelCatalogEnvelope.self, from: data)
            if let error = envelope.error {
                return .failed(.decoding(error))
            }
            guard let result = envelope.result else {
                return .failed(.invalidResponse)
            }
            return .loaded(result.models)
        } catch {
            return .failed(.decoding(error.localizedDescription))
        }
    }

    private struct LocalModelCatalogEnvelope: Decodable {
        let result: LocalModelCatalogResult?
        let error: String?
    }

    private struct LocalModelCatalogResult: Decodable {
        let models: [LocalModelSpec]
    }
}
