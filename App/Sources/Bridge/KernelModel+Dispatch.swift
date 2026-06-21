import Foundation
import os.log

// MARK: - KernelModel dispatch, toast, and scene-lifecycle
//
// Grouped here because all three concerns share the same
// fire-and-forget pattern and surface failures through the same
// `lastErrorToast` channel. Extracted from KernelModel.swift to keep
// that file under the AGENTS.md 500-line hard limit.

extension KernelModel {

    // ── scenePhase lifecycle ───────────────────────────────────────────────

    func lifecycleForeground() {
        kernel.lifecycleForeground()
        guard hasObservedForeground else {
            hasObservedForeground = true
            // Cold start skips `RefreshAll` (the snapshot already loaded from
            // disk), so the fresh-feed auto-download path never runs at launch.
            // Kick a catch-up evaluation over the current library so enabled
            // shows still pull their latest undownloaded episodes without
            // waiting for a manual pull-to-refresh.
            _ = dispatch(namespace: "podcast", body: ["op": "auto_download_evaluate"])
            return
        }
        dispatch(PodcastKernelAction.RefreshAll())
    }

    func lifecycleBackground() { kernel.lifecycleBackground() }

    // ── Toast ──────────────────────────────────────────────────────────────

    func clearErrorToast() { lastErrorToast = nil }

    /// Set the toast surface from outside this file. Used by features
    /// (notably `Features/Identity/IdentityViewModel.swift`) that need to
    /// route a staged-action notice through the same banner channel as
    /// synchronous dispatch failures.
    ///
    /// `lastErrorToast` has an internal setter; callers in other files can
    /// also assign it directly, but this named entry point is preferred for
    /// intent clarity.
    func setErrorToast(_ message: String?) {
        lastErrorToast = message
    }

    // ── Dispatch ───────────────────────────────────────────────────────────

    /// Fire-and-forget generic dispatch. Surfaces synchronous rejections as a
    /// toast (D6 — outcomes always arrive in-band; never throws).
    @discardableResult
    func dispatch(namespace: String, body: [String: Any]) -> DispatchResult {
        let result = kernel.dispatchAction(namespace: namespace, body: body)
        if case let .failure(message) = result {
            kmLog.error("dispatch_action rejected: \(message, privacy: .public)")
            lastErrorToast = message
        }
        // Surface the result of the user action without waiting for the next
        // push frame. The full-library decode runs off the MainActor (see
        // `pullPodcastSnapshotIfChanged`) so this dispatch returns immediately;
        // the projection commits a runloop later — no caller depends on a
        // same-runloop read of `library`/`podcastSnapshot`/`episodes`.
        pullPodcastSnapshotIfChanged()
        return result
    }

    /// Identical to `dispatch(namespace:body:)` but logs failures instead of
    /// surfacing them as a user-visible toast. For callers that are
    /// best-effort and where a transient rejection (e.g. an action the
    /// kernel has not registered yet) is expected and should stay
    /// developer-only. Today the only caller is `iCloudSyncCapability`,
    /// which dispatches `podcast.settings.*` actions whose Rust handlers
    /// land in a follow-up PR.
    @discardableResult
    func dispatchSilent(namespace: String, body: [String: Any]) -> DispatchResult {
        let result = kernel.dispatchAction(namespace: namespace, body: body)
        if case let .failure(message) = result {
            kmLog.error("dispatch_action (silent) rejected: \(message, privacy: .public)")
        }
        // Surface the result off-main, same as `dispatch`.
        pullPodcastSnapshotIfChanged()
        return result
    }
}

private let kmLog = Logger(subsystem: "io.f7z.podcast", category: "KernelModel")
