import Foundation
// LiteRTLM's Engine/Conversation types predate Swift 6 strict-concurrency
// auditing (Conversation is a non-Sendable class). @preconcurrency downgrades
// the cross-actor non-Sendable diagnostics to warnings for this SDK.
@preconcurrency import LiteRTLM
import os.log

actor LocalLLMService {
    private var engine: Engine?

    /// The model id whose engine is currently loaded, or nil when none. Lets
    /// `ensureLoaded` skip a redundant (expensive) re-init when the same model
    /// is requested again, and reload when the selection changes.
    private(set) var loadedModelID: String?

    /// Tail of the serialized load chain. Each `ensureLoaded` chains after this
    /// so concurrent calls can't overlap `load`'s multi-second GPU init.
    private var pendingLoad: Task<Void, Error>?

    private nonisolated var cacheDir: URL {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        return caches.appendingPathComponent("LiteRTCache", isDirectory: true)
    }

    init() {
        try? FileManager.default.createDirectory(at: cacheDir, withIntermediateDirectories: true)
    }

    /// Loads `spec`'s engine unless it is already the loaded one. Switching
    /// models unloads the previous engine first (only one on-device engine is
    /// kept resident at a time).
    ///
    /// Concurrency-safe: loads are serialized through `pendingLoad`. Without
    /// this, two near-simultaneous calls (a settings change racing kernel
    /// attach, or a fast X→Y→X switch) could both pass the `loadedModelID`
    /// check before either finished `load`'s slow GPU init and double-init —
    /// leaving an engine that no longer matches the latest selection. Chaining
    /// makes the most-recently-requested model the resident one.
    func ensureLoaded(spec: LocalModelSpec, downloadManager: LocalModelDownloadManager) async throws {
        if loadedModelID == spec.id, engine != nil { return }
        let previous = pendingLoad
        let task = Task { [weak self] in
            _ = try? await previous?.value
            guard let self else { return }
            try await self.loadSerialized(spec: spec, downloadManager: downloadManager)
        }
        pendingLoad = task
        try await task.value
    }

    /// Runs inside the serialized load chain: re-checks (a prior chained load
    /// may have already brought this model up), then swaps the resident engine.
    private func loadSerialized(spec: LocalModelSpec, downloadManager: LocalModelDownloadManager) async throws {
        if loadedModelID == spec.id, engine != nil { return }
        engine = nil
        loadedModelID = nil
        try await load(spec: spec, downloadManager: downloadManager)
    }

    func load(spec: LocalModelSpec, downloadManager: LocalModelDownloadManager) async throws {
        guard let fileURL = await downloadManager.modelFileURL(for: spec.id) as URL? else {
            throw LocalLLMError.modelNotFound
        }

        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            throw LocalLLMError.modelFileNotFound
        }

        let config = try EngineConfig(
            modelPath: fileURL.path,
            backend: .gpu,
            cacheDir: cacheDir.path
        )
        let newEngine = Engine(engineConfig: config)
        try await newEngine.initialize()
        engine = newEngine
        loadedModelID = spec.id
        os_log("Local LLM engine initialized with model: %{public}@", log: .default, type: .info, fileURL.path)
    }

    func unload() async {
        engine = nil
        loadedModelID = nil
    }

    func infer(promptJSON: String) async -> String {
        guard let engine = engine, let resident = loadedModelID else {
            return #"{"error":"Local model not loaded"}"#
        }

        // Extract the prompt text from the JSON payload sent by the Rust kernel.
        // The kernel's LocalModelBackend sends {"system","history","user","model"}
        // where history is an array of [role, content] pairs. We also accept
        // {"prompt"} and {"messages":[…]} shapes defensively.
        var promptText = promptJSON
        var requestedModel: String?
        if let data = promptJSON.data(using: .utf8),
           let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            requestedModel = obj["model"] as? String
            if let p = obj["prompt"] as? String {
                promptText = p
            } else if let messages = obj["messages"] as? [[String: Any]],
                      let last = messages.last,
                      let content = last["content"] as? String {
                promptText = content
            } else if obj["user"] != nil || obj["system"] != nil || obj["history"] != nil {
                // Compose the kernel's structured request into a single prompt:
                // system preamble, prior turns, then the new user message.
                var parts: [String] = []
                if let system = obj["system"] as? String, !system.isEmpty {
                    parts.append(system)
                }
                if let history = obj["history"] as? [[Any]] {
                    for turn in history where turn.count == 2 {
                        if let role = turn[0] as? String, let content = turn[1] as? String {
                            parts.append("\(role): \(content)")
                        }
                    }
                }
                if let user = obj["user"] as? String, !user.isEmpty {
                    parts.append("User: \(user)")
                }
                if !parts.isEmpty {
                    promptText = parts.joined(separator: "\n\n")
                }
            }
        }

        // The kernel routes each role to its own LocalModelBackend{model_id},
        // but only one engine is resident at a time. If a role asks for a model
        // that isn't the loaded one, refuse rather than silently answering with
        // the wrong model — the kernel maps this error to Unavailable.
        if let requestedModel, requestedModel != resident {
            let safe = requestedModel.replacingOccurrences(of: "\"", with: "'")
            return "{\"error\":\"requested model \(safe) is not the resident on-device model (\(resident))\"}"
        }

        do {
            let conversation = try await engine.createConversation()
            let userMessage = Message(contents: [.text(promptText)], role: .user)
            let response = try await conversation.sendMessage(userMessage)

            // Extract text from the response message
            let responseText = response.contents.compactMap { content -> String? in
                if case .text(let t) = content { return t }
                return nil
            }.joined()

            guard let jsonData = try? JSONSerialization.data(
                withJSONObject: ["text": responseText]),
                  let jsonStr = String(data: jsonData, encoding: .utf8) else {
                return #"{"text":""}"#
            }
            return jsonStr
        } catch {
            os_log("Local LLM inference error: %{public}@", log: .default, type: .error,
                   error.localizedDescription)
            let msg = error.localizedDescription.replacingOccurrences(of: "\"", with: "'")
            return "{\"error\":\"\(msg)\"}"
        }
    }

    func registerWithKernel(_ kernel: KernelModel) async {
        let handleBits = await MainActor.run { Int(bitPattern: kernel.podcastHandlePointer) }
        guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
            os_log("Cannot register local LLM: no kernel handle", log: .default, type: .error)
            return
        }

        let ctx = Unmanaged.passUnretained(self).toOpaque()
        nmp_app_register_local_llm(handle, ctx, localLLMCallback)
        os_log("Local LLM service registered with kernel", log: .default, type: .debug)
    }

    func clearFromKernel(_ kernel: KernelModel) async {
        let handleBits = await MainActor.run { Int(bitPattern: kernel.podcastHandlePointer) }
        guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else { return }
        nmp_app_clear_local_llm(handle)
        os_log("Local LLM service cleared from kernel", log: .default, type: .debug)
    }
}

// MARK: - C Callback Glue

private func localLLMCallback(
    _ context: UnsafeMutableRawPointer?,
    _ promptJSON: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let context = context else { return nil }
    guard let promptJSON = promptJSON else { return nil }

    let service = Unmanaged<LocalLLMService>.fromOpaque(context).takeUnretainedValue()
    let promptString = String(cString: promptJSON)

    // The FFI call runs on a Rust background thread (not the cooperative pool),
    // so blocking with a semaphore here is safe. The semaphore guarantees the
    // Task's write to `box` happens-before the read after `wait()`, so the
    // @unchecked Sendable box carries the result across the boundary without a
    // real race (Swift 6 can't prove the ordering, hence the box).
    final class ResultBox: @unchecked Sendable {
        var value = #"{"error":"Inference failed"}"#
    }
    let box = ResultBox()
    let semaphore = DispatchSemaphore(value: 0)

    let task = Task {
        box.value = await service.infer(promptJSON: promptString)
        semaphore.signal()
    }

    semaphore.wait()
    task.cancel()

    guard let resultCString = box.value.cString(using: .utf8) else {
        return nil
    }

    return strdup(resultCString)
}

// MARK: - Error Types

enum LocalLLMError: Error {
    case modelNotFound
    case modelFileNotFound
    case engineInitializationFailed
}
