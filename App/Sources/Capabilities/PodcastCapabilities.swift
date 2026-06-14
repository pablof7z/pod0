import Foundation

/// Capability injection point for Podcast.
///
/// The kernel grants the app a set of capability *sockets*; the app supplies
/// the platform implementation. This holder is the one place those
/// implementations are constructed and started, mirroring the thin-bridge
/// pattern in `Bridge/KernelBridge.swift`.
///
/// It owns:
///   - `KeychainCapability`    — generic NMP keyring (`nmp.keyring.capability`)
///   - `PcstIdentityCapability` — podcast-app identity/BYOK slots (`pcst.identity.capability`)
///   - `HttpCapability`        — host HTTP transport
///   - `AudioCapability`       — AVPlayer-backed audio (`nmp.audio.capability`)
///   - `DownloadCapability`    — URLSession background downloads
///                                (`nmp.download.capability`)
///   - `NotificationCapability` — UNUserNotificationCenter scheduler
///                                (`nmp.notification.capability`). Feature
///                                #20: new-episode local notifications.
///   - `PlatformCapability`    — passive platform-integration executor
///                                (widget App Group writes, NSUserActivity
///                                donations, …). Driven by snapshot ticks,
///                                not by kernel request/response — so it is
///                                **not** routed through `handleJSON(_:)`,
///                                but it shares the start/stop lifecycle of
///                                the other capabilities.
///   - `SpotlightCapability`   — passive CoreSpotlight indexer
///                                (`pcst.spotlight.capability`). Driven by
///                                `KernelModel.library` deltas in the
///                                snapshot bridge; same passive shape as
///                                `PlatformCapability`. Held via
///                                `SpotlightCapability.shared` so the
///                                deep-link router can reach it from
///                                `PodcastApp` without re-injecting it.
///
/// Rust decides when and what to call; Swift only executes the request and
/// reports the raw result (D7).
///
/// There is a single C capability callback (`nmp_app_set_capability_callback`);
/// it routes by the `namespace` field of the incoming `CapabilityRequest` —
/// see [`handleJSON(_:)`].
@MainActor
final class PodcastCapabilities {
    /// Process-wide instance.
    ///
    /// Background `URLSession` delegate events can arrive before SwiftUI's
    /// `@State` initialisers run (notably when the OS relaunches the app
    /// into the background to drain an in-flight download), so the
    /// capability holder must be reachable from the
    /// `UIApplicationDelegate` hook regardless of view-graph state. The
    /// `PodcastAppDelegate` forwards
    /// `application(_:handleEventsForBackgroundURLSession:completionHandler:)`
    /// into `shared.download.handleEventsForBackgroundURLSession(...)`.
    ///
    /// The iOS-side capability surface owns the OS hook regardless of where
    /// the kernel wires it.
    static let shared = PodcastCapabilities()

    let keyring: KeychainCapability
    let identity: PcstIdentityCapability
    let http: HttpCapability
    let audio: AudioCapability
    let download: DownloadCapability
    let notification: NotificationCapability
    let platform: PlatformCapability
    /// iCloud settings sync (`pcst.icloud_sync.capability`). Passive —
    /// driven by KVS notifications and the snapshot bridge, not by
    /// kernel-issued requests. Started separately from `start()` because
    /// the inbound dispatch path needs a `KernelModel` reference; see
    /// `startICloudSync(kernel:)`.
    let iCloudSync: iCloudSyncCapability
    let spotlight: SpotlightCapability
    let voice: VoiceCapability
    /// Network-state monitor. Observes `NWPathMonitor` and delivers
    /// `nmp.network.capability` `ConnectivityChanged` reports so Rust's
    /// `is_on_wifi` flag stays current for Wi-Fi-only auto-download gating.
    /// Started from `KernelModel.init()` after the podcast projection is
    /// registered (it needs the podcast handle to send reports).
    let network: NetworkCapability

    init(
        keyring: KeychainCapability = KeychainCapability(),
        identity: PcstIdentityCapability = PcstIdentityCapability(),
        http: HttpCapability = HttpCapability(),
        audio: AudioCapability = AudioCapability(),
        download: DownloadCapability = DownloadCapability(),
        notification: NotificationCapability = NotificationCapability(),
        platform: PlatformCapability = PlatformCapability(),
        iCloudSync: iCloudSyncCapability = iCloudSyncCapability(),
        spotlight: SpotlightCapability = SpotlightCapability.shared,
        voice: VoiceCapability = VoiceCapability(),
        network: NetworkCapability = NetworkCapability()
    ) {
        self.keyring = keyring
        self.identity = identity
        self.http = http
        self.audio = audio
        self.download = download
        self.notification = notification
        self.platform = platform
        self.iCloudSync = iCloudSync
        self.spotlight = spotlight
        self.voice = voice
        self.network = network
    }

    /// Idempotent: start all owned capabilities. Safe to call on every app
    /// foreground.
    ///
    /// Note: `iCloudSync` is **not** started from here because it needs a
    /// `KernelModel` to dispatch inbound actions through. Callers (today
    /// only `PodcastApp.body.task`) invoke `startICloudSync(kernel:)`
    /// after `model.start()` has been issued.
    func start() {
        keyring.start()
        identity.start()
        http.start()
        audio.start()
        download.start()
        notification.start()
        platform.start()
        spotlight.start()
    }

    /// Bring the iCloud sync capability online. Idempotent. Separated
    /// from `start()` so the model can be wired in after the kernel has
    /// booted and the capability has somewhere to dispatch.
    func startICloudSync(kernel: KernelModel, appStore: AppStateStore) {
        iCloudSync.appStore = appStore
        iCloudSync.start(kernel: kernel)
        // Give the voice executor read access to the projected settings so
        // `Speak` can route to the selected TTS provider (D7: iOS reads the
        // kernel-owned projection to pick an executor path).
        voice.appStore = appStore
        voice.start()
    }

    /// Idempotent: mark capabilities inactive. Does not erase stored secrets.
    func stop() {
        keyring.stop()
        identity.stop()
        http.stop()
        audio.stop()
        download.stop()
        notification.stop()
        platform.stop()
        iCloudSync.stop()
        spotlight.stop()
        voice.stop()
    }

    /// Single capability-callback entry point. Routes the raw kernel
    /// `CapabilityRequest` JSON to the capability owning its `namespace` and
    /// returns the raw `CapabilityEnvelope` JSON.
    ///
    /// D6: an unparseable request or an unknown namespace yields a populated
    /// error envelope string, never a thrown error and never `nil`.
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            // Cannot even read the namespace — return a generic error envelope.
            let env = CapabilityEnvelope(
                namespace: "",
                correlationID: "",
                resultJSON: "{\"status\":\"error\",\"message\":\"malformed-request\"}")
            return Self.encode(env) ?? "{}"
        }

        switch request.namespace {
        case KeychainCapability.namespace:
            return keyring.handleJSON(requestJSON)
        case PcstIdentityCapability.namespace:
            return identity.handleJSON(requestJSON)
        case HttpCapability.namespace:
            return http.handleJSON(requestJSON)
        case AudioCapability.namespace:
            return audio.handleJSON(requestJSON)
        case DownloadCapability.namespace:
            return download.handleJSON(requestJSON)
        case NotificationCapability.namespace:
            return notification.handleJSON(requestJSON)
        case VoiceCapability.namespace:
            return voice.handleJSON(requestJSON)
        default:
            // D6 — an unknown namespace is data, not a crash. Echo the
            // correlation id so the issuing kernel module can still correlate.
            let env = CapabilityEnvelope(
                namespace: request.namespace,
                correlationID: request.correlationID,
                resultJSON: "{\"status\":\"error\",\"message\":\"unknown-namespace\"}")
            return Self.encode(env) ?? "{}"
        }
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
