import AVFoundation
import Foundation

enum PlaybackRouteChangeReason: String, Equatable, Sendable {
    case newDeviceAvailable
    case oldDeviceUnavailable
    case categoryChange
    case override
    case wakeFromSleep
    case noSuitableRoute
    case routeConfigurationChange
    case unknown
}

enum PlaybackAudioSessionEvent: Equatable, Sendable {
    case interruptionBegan(route: NativePlaybackAudioRoute)
    case interruptionEnded(shouldResume: Bool, route: NativePlaybackAudioRoute)
    case routeChanged(
        reason: PlaybackRouteChangeReason,
        previous: NativePlaybackAudioRoute,
        current: NativePlaybackAudioRoute
    )
    case mediaServicesWereReset(route: NativePlaybackAudioRoute)
}

/// Converts process-wide AVAudioSession notifications into bounded typed host
/// events. The Rust playback policy owns shared-mode pause/resume decisions;
/// PlaybackState retains only the pre-cutover fallback path.
@MainActor
final class PlaybackAudioSessionObserver {
    private let notificationCenter: NotificationCenter
    private let session: AVAudioSession
    private var tokens: [NSObjectProtocol] = []

    var onEvent: (PlaybackAudioSessionEvent) -> Void = { _ in }

    init(
        notificationCenter: NotificationCenter = .default,
        session: AVAudioSession = .sharedInstance()
    ) {
        self.notificationCenter = notificationCenter
        self.session = session
        start()
    }

    func stop() {
        tokens.forEach(notificationCenter.removeObserver)
        tokens.removeAll()
    }

    private func start() {
        observe(AVAudioSession.interruptionNotification)
        observe(AVAudioSession.routeChangeNotification)
        observe(AVAudioSession.mediaServicesWereResetNotification)
    }

    private func observe(_ name: Notification.Name) {
        let token = notificationCenter.addObserver(
            forName: name,
            object: session,
            queue: .main
        ) { [weak self] notification in
            guard let currentRoute = MainActor.assumeIsolated({ self?.currentRoute }),
                  let event = Self.event(from: notification, currentRoute: currentRoute)
            else { return }
            MainActor.assumeIsolated { self?.onEvent(event) }
        }
        tokens.append(token)
    }

    private var currentRoute: NativePlaybackAudioRoute {
        Self.route(for: session.currentRoute.outputs.map(\.portType))
    }

    nonisolated static func event(
        from notification: Notification,
        currentRoute: NativePlaybackAudioRoute
    ) -> PlaybackAudioSessionEvent? {
        switch notification.name {
        case AVAudioSession.interruptionNotification:
            guard let rawType = number(
                in: notification.userInfo,
                key: AVAudioSessionInterruptionTypeKey
            ), let type = AVAudioSession.InterruptionType(rawValue: rawType)
            else { return nil }
            switch type {
            case .began:
                return .interruptionBegan(route: currentRoute)
            case .ended:
                let rawOptions = number(
                    in: notification.userInfo,
                    key: AVAudioSessionInterruptionOptionKey
                ) ?? 0
                let shouldResume = AVAudioSession.InterruptionOptions(rawValue: rawOptions)
                    .contains(.shouldResume)
                return .interruptionEnded(shouldResume: shouldResume, route: currentRoute)
            @unknown default:
                return nil
            }

        case AVAudioSession.routeChangeNotification:
            let rawReason = number(
                in: notification.userInfo,
                key: AVAudioSessionRouteChangeReasonKey
            ) ?? 0
            let reason = routeReason(AVAudioSession.RouteChangeReason(rawValue: rawReason))
            let previousDescription = notification.userInfo?[AVAudioSessionRouteChangePreviousRouteKey]
                as? AVAudioSessionRouteDescription
            let previous = route(for: previousDescription?.outputs.map(\.portType) ?? [])
            return .routeChanged(reason: reason, previous: previous, current: currentRoute)

        case AVAudioSession.mediaServicesWereResetNotification:
            return .mediaServicesWereReset(route: currentRoute)

        default:
            return nil
        }
    }

    nonisolated static func route(for ports: [AVAudioSession.Port]) -> NativePlaybackAudioRoute {
        guard !ports.isEmpty else { return .unknown }
        if ports.contains(where: { [.headphones, .headsetMic, .lineOut].contains($0) }) {
            return .wired
        }
        if ports.contains(where: { [.bluetoothA2DP, .bluetoothHFP, .bluetoothLE].contains($0) }) {
            return .bluetooth
        }
        if ports.contains(.airPlay) { return .airPlay }
        if ports.contains(.carAudio) { return .car }
        if ports.contains(where: { [.builtInReceiver, .builtInSpeaker].contains($0) }) {
            return .builtIn
        }
        return .external
    }

    nonisolated private static func routeReason(
        _ reason: AVAudioSession.RouteChangeReason?
    ) -> PlaybackRouteChangeReason {
        switch reason {
        case .newDeviceAvailable: .newDeviceAvailable
        case .oldDeviceUnavailable: .oldDeviceUnavailable
        case .categoryChange: .categoryChange
        case .override: .override
        case .wakeFromSleep: .wakeFromSleep
        case .noSuitableRouteForCategory: .noSuitableRoute
        case .routeConfigurationChange: .routeConfigurationChange
        case .unknown, .none: .unknown
        @unknown default: .unknown
        }
    }

    nonisolated private static func number(
        in userInfo: [AnyHashable: Any]?,
        key: String
    ) -> UInt? {
        (userInfo?[key] as? NSNumber)?.uintValue
    }
}

extension AudioEngine {
    func configureAudioSessionObserver() {
        let observer = PlaybackAudioSessionObserver()
        observer.onEvent = { [weak self] event in
            guard let self else { return }
            self.onHostAudioSessionEvent(event)
        }
        audioSessionObserver = observer
    }
}
