import Pod0Core

/// Lets the shared facade bootstrap before SwiftUI creates its one
/// `PlaybackState`. The proxy owns no playback facts; once attached it merely
/// forwards typed host requests and raw observations to the AVFoundation host.
@MainActor
final class DeferredPlaybackHost: CorePlaybackHosting {
    private var host: (any CorePlaybackHosting)?
    private var observationSink: (PlaybackLifecycleObservation) -> Void = { _ in }

    func attach(_ host: any CorePlaybackHosting) {
        self.host = host
        host.installObservationSink(observationSink)
    }

    func execute(_ request: HostRequest) -> HostObservation {
        guard let host else {
            return .failed(
                code: .mediaUnavailable,
                safeDetail: "Playback host is not attached"
            )
        }
        return host.execute(request)
    }

    func installObservationSink(
        _ sink: @escaping (PlaybackLifecycleObservation) -> Void
    ) {
        observationSink = sink
        host?.installObservationSink(sink)
    }
}
