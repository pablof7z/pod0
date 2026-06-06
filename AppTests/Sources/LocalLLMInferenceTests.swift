import XCTest
@testable import Podcastr

/// Device-only integration probe for on-device LiteRT-LM inference.
///
/// Runs the EXACT production path the agent uses (`LocalLLMService.load` +
/// `infer`) against the downloaded Gemma 4 E2B weights, so the precise native
/// failure surfaces in the test log instead of needing a device `os_log`
/// capture. Skips automatically when the model isn't downloaded or when run on
/// a simulator (no real GPU for the `.gpu` backend).
final class LocalLLMInferenceTests: XCTestCase {

    func testGemmaE2BInference() async throws {
        let modelID = "gemma4-e2b"

        #if targetEnvironment(simulator)
        throw XCTSkip("LiteRT-LM .gpu backend needs a real device — run on the iPhone")
        #endif

        // Opt-in: this test loads its own ~2.6 GB engine. To avoid a second
        // resident engine (the host app auto-loads one too → ~5 GB → killed),
        // run with the app's auto-load disabled:
        //   TEST_RUNNER_DISABLE_LOCAL_ENGINE_AUTOLOAD=1 xcodebuild test ...
        guard ProcessInfo.processInfo.environment["DISABLE_LOCAL_ENGINE_AUTOLOAD"] == "1" else {
            throw XCTSkip("Set TEST_RUNNER_DISABLE_LOCAL_ENGINE_AUTOLOAD=1 to run the single-engine probe")
        }

        let fileURL = DownloadCapability.localModelFileURL(for: modelID)
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            throw XCTSkip("Model not present at \(fileURL.path) — download Gemma 4 E2B on this device first")
        }
        let attrs = try? FileManager.default.attributesOfItem(atPath: fileURL.path)
        let size = (attrs?[.size] as? NSNumber)?.int64Value ?? -1
        print("LLMTEST: model file present path=\(fileURL.path) sizeBytes=\(size)")

        guard let spec = LocalModelCatalog.all.first(where: { $0.id == modelID }) else {
            return XCTFail("LLMTEST: no catalog spec for \(modelID)")
        }

        let service = LocalLLMService()

        // 1) Engine load (EngineConfig + initialize). If this throws, the failure
        //    is in model loading / GPU init, not inference.
        do {
            try await service.load(spec: spec)
            print("LLMTEST: engine.load OK (resident=\(modelID))")
        } catch {
            return XCTFail("LLMTEST: engine.load FAILED: \(error)")
        }

        // 2) Trivial prompt — does basic inference work at all?
        let tiny = #"{"system":"You are a helpful assistant.","history":[],"user":"Say hello in one word.","model":"gemma4-e2b"}"#
        let tinyOut = await service.infer(promptJSON: tiny)
        print("LLMTEST: [tiny prompt] result=\(tinyOut)")

        // 3) Large prompt — does a big system preamble (like the agent's prompt +
        //    tool instructions) break the native layer (context overflow)?
        let bigSystem = String(repeating: "You are an AI agent for a podcast app with tools. ", count: 300)
        let bigPrompt = "{\"system\":\"\(bigSystem)\",\"history\":[],\"user\":\"What can you do?\",\"model\":\"gemma4-e2b\"}"
        print("LLMTEST: [large prompt] systemChars=\(bigSystem.count)")
        let bigOut = await service.infer(promptJSON: bigPrompt)
        print("LLMTEST: [large prompt] result=\(bigOut)")

        // The test's value is the printed diagnostics above; assert the tiny path
        // so a green/red signal is also captured.
        XCTAssertFalse(tinyOut.contains("\"error\""),
                       "LLMTEST: tiny inference returned an error: \(tinyOut)")
    }

    /// Drives the REAL agent flow end-to-end (the exact path the user's chat
    /// uses): `AgentLLMClient.streamCompletion` → kernel `chat_complete` →
    /// `single_turn` → the on-device LocalModelBackend → the app's auto-loaded
    /// engine. Uses the app's OWN engine (auto-load left enabled) so there is no
    /// second resident engine — this reproduces normal usage exactly.
    func testAgentChatEndToEndWithLocalModel() async throws {
        #if targetEnvironment(simulator)
        throw XCTSkip("on-device agent + GPU model — run on the iPhone")
        #endif
        guard ProcessInfo.processInfo.environment["DISABLE_LOCAL_ENGINE_AUTOLOAD"] != "1" else {
            throw XCTSkip("run WITHOUT DISABLE_LOCAL_ENGINE_AUTOLOAD so the app loads its engine")
        }
        guard FileManager.default.fileExists(
            atPath: DownloadCapability.localModelFileURL(for: "gemma4-e2b").path) else {
            throw XCTSkip("Gemma 4 E2B not downloaded on this device — download it first")
        }

        // Wait for the kernel to attach (app boot). Only read a Bool across the
        // actor hop — the raw handle pointer is not Sendable.
        var waited = 0.0
        var attached = false
        while waited < 30 {
            attached = await MainActor.run { KernelModel.shared?.podcastHandlePointer != nil }
            if attached { break }
            try await Task.sleep(for: .seconds(1)); waited += 1
        }
        guard attached else { return XCTFail("AGENTTEST: kernel never attached") }
        print("AGENTTEST: kernel attached after \(waited)s")

        // The engine loads asynchronously after launch; while it's still loading
        // the backend reports "not loaded" — poll until it resolves either way.
        var reply: String?
        var lastError = ""
        var elapsed = 0.0
        let deadline = 150.0
        while elapsed < deadline {
            do {
                // Build the message inline (fresh, used-once) so Swift 6 region
                // isolation can "send" the non-Sendable [[String:Any]] safely.
                let result = try await AgentLLMClient.streamCompletion(
                    messages: [["role": "user", "content": "Say hello in one short sentence."]],
                    tools: [], model: "") { _ in }
                reply = result.assistantMessage["content"] as? String
                print("AGENTTEST: reply=\(reply ?? "<nil>")")
                break
            } catch {
                lastError = error.localizedDescription
                if lastError.localizedCaseInsensitiveContains("not loaded") {
                    print("AGENTTEST: engine still loading (\(lastError)) — retrying…")
                    try await Task.sleep(for: .seconds(3)); elapsed += 3
                    continue
                }
                print("AGENTTEST: agent FAILED with real error: \(lastError)")
                break
            }
        }

        XCTAssertNotNil(reply, "AGENTTEST: agent never replied; last error: \(lastError)")
        XCTAssertFalse((reply ?? "").isEmpty, "AGENTTEST: agent reply was empty")
    }
}
