import Foundation
import os.log

// MARK: - AppStateStore + Local on-device LLM engine lifecycle
//
// PR #253 made "Local" a per-role provider: a role's model string is
// `local:<id>`, and the kernel's `backend_for` routes that role to the
// on-device LocalModelBackend, which calls back into Swift's LocalLLMService.
// But the callback only yields output once an engine is actually loaded — and
// nothing on main loaded one (PR #253 called this out as UI-ahead-of-backend).
//
// This is the missing reaction: it keeps exactly one on-device engine resident,
// matching the single `effectiveLocalModelID` derived from the role selections.
// It runs on every settings change and once at kernel attach.

extension AppStateStore {

    /// Load / unload the on-device LLM engine to match the local model derived
    /// from role selections (`effectiveLocalModelID`). Idempotent.
    ///
    /// - Target nil (no role uses a local model): unload any resident engine.
    /// - Target set + file present: load it (skipped if already loaded).
    /// - Target set + file missing: start a download. The engine loads on the
    ///   next sync (settings change or relaunch) once the file is present. The
    ///   kernel download-unification work will close this into an auto-load on
    ///   download completion.
    func syncLocalEngine(for settings: Settings) {
        // Test hook: let an integration test load EXACTLY ONE engine itself
        // without the app racing in a second resident engine (two ~2.6 GB
        // engines blow the memory budget — a test artifact, not app behavior).
        if ProcessInfo.processInfo.environment["DISABLE_LOCAL_ENGINE_AUTOLOAD"] == "1" {
            return
        }
        let service = localLLMService

        guard let targetID = Self.effectiveLocalModelID(settings) else {
            Task { await service.unload() }
            return
        }

        guard let spec = LocalModelCatalog.all.first(where: { $0.id == targetID }) else {
            os_log("syncLocalEngine: no catalog spec for local model id %{public}@",
                   log: .default, type: .error, targetID)
            return
        }

        let fileURL = DownloadCapability.localModelFileURL(for: targetID)
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            // Not downloaded yet. Queue it through the unified download queue —
            // but only if it isn't already in flight (this method runs on every
            // settings change; the kernel queue is also idempotent per id). The
            // engine loads on the next sync once the file lands.
            if localModelDownloads[targetID] != nil {
                os_log("syncLocalEngine: model %{public}@ download already in flight",
                       log: .default, type: .info, targetID)
            } else {
                os_log("syncLocalEngine: model %{public}@ not downloaded yet — queuing download",
                       log: .default, type: .info, targetID)
                kernelDownloadLocalModel(modelID: targetID, url: spec.downloadURL.absoluteString)
            }
            return
        }

        Task {
            do {
                try await service.ensureLoaded(spec: spec)
            } catch {
                os_log("syncLocalEngine: failed to load local model %{public}@: %{public}@",
                       log: .default, type: .error, targetID, error.localizedDescription)
            }
        }
    }
}
