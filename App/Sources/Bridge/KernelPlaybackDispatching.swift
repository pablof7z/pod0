import Foundation

// MARK: - KernelPlaybackDispatching

/// Protocol capturing the subset of `AppStateStore` playback transport
/// dispatch methods consumed by `PlaybackState`.
///
/// Extracted from the concrete `AppStateStore` (which is `final` and uses
/// extension-defined methods) so that `PlaybackState` can accept a
/// lightweight stub in unit tests without subclassing the store.
///
/// Production wiring: `AppStateStore` conforms via the implementations in
/// `AppStateStore+KernelActions.swift`. Set `PlaybackState.kernelDispatch`
/// to `nil` in production (the default); `PlaybackState` then falls back to
/// `store`. Inject a conforming stub into `kernelDispatch` in tests.
@MainActor
protocol KernelPlaybackDispatching: AnyObject {
    /// Stage `episodeID` in the Rust player actor without starting playback.
    /// Must be called before `kernelResume()` whenever the active episode changes.
    func kernelLoad(episodeID: UUID)

    /// Load and start `episodeID` through Rust-owned playback policy.
    @discardableResult
    func kernelPlay(episodeID: UUID, startSeconds: Double?, endSeconds: Double?) -> DispatchResult?

    /// Resume playback of the currently-staged episode.
    func kernelResume()

    /// Pause playback.
    @discardableResult
    func kernelPause() -> DispatchResult?

    /// Seek to `positionSecs`.
    @discardableResult
    func kernelSeek(positionSecs: Double) -> DispatchResult?

    /// Skip forward by `secs` from the kernel's current position.
    @discardableResult
    func kernelSkipForward(secs: Double?) -> DispatchResult?

    /// Skip backward by `secs` from the kernel's current position.
    @discardableResult
    func kernelSkipBackward(secs: Double?) -> DispatchResult?

    /// Set playback speed through the Rust player actor.
    @discardableResult
    func kernelSetSpeed(_ speed: Double) -> DispatchResult?
}

// MARK: - AppStateStore conformance

extension AppStateStore: KernelPlaybackDispatching {}
