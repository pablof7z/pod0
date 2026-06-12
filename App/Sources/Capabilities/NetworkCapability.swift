import Foundation
import Network

/// Observes `NWPathMonitor` and fires `nmp.network.capability`
/// `ConnectivityChanged` reports so the Rust kernel knows whether
/// the device is on Wi-Fi. Rust uses this to honour per-subscription
/// Wi-Fi-only auto-download policies without iOS deciding which
/// episodes to queue (D7).
///
/// When Wi-Fi is restored, also dispatches
/// `podcast.dispatch_deferred_wifi_downloads` so episodes that were deferred
/// during a cellular-only session are downloaded immediately.
///
/// Usage: call `start(handle:onWifiRestored:)` once from `KernelModel.init()`.
@MainActor
final class NetworkCapability {
    private let monitor = NWPathMonitor()
    private let monitorQueue = DispatchQueue(label: "podcast.network-monitor", qos: .utility)
    /// Intentionally starts as `false` so the very first `NWPathMonitor` update
    /// — which arrives even when the device was already on Wi-Fi at launch —
    /// always triggers `onWifiRestored` when Wi-Fi is confirmed. This ensures
    /// persisted deferred downloads (from a previous session on cellular) are
    /// dispatched immediately on a cold launch that starts on Wi-Fi, rather than
    /// being stuck until the user cycles between networks.
    /// The Rust `is_on_wifi` default (`true`) is intentionally conservative and
    /// independent of this Swift flag.
    private(set) var isOnWifi = false
    /// Called on the main actor when Wi-Fi is (re)connected.
    var onWifiRestored: (() -> Void)?

    /// Begin monitoring and send an initial `ConnectivityChanged` event
    /// to prime the Rust-side state. No-op when `handle` is nil.
    func start(handle: UnsafeMutableRawPointer?) {
        guard let handle else { return }
        let rawHandle = UInt(bitPattern: handle)
        monitor.pathUpdateHandler = { [weak self] path in
            let wifi = path.usesInterfaceType(.wifi)
            let connected = path.status == .satisfied
            Task { @MainActor [weak self] in
                guard let self,
                      let ptr = UnsafeMutableRawPointer(bitPattern: rawHandle)
                else { return }
                let wasOnWifi = self.isOnWifi
                self.isOnWifi = wifi
                self.sendReport(
                    NetworkReport(isWifi: wifi, isConnected: connected),
                    handle: ptr)
                // When transitioning from cellular to Wi-Fi, drain deferred downloads.
                if wifi && connected && !wasOnWifi {
                    self.onWifiRestored?()
                }
            }
        }
        monitor.start(queue: monitorQueue)
    }

    func stop() {
        monitor.cancel()
        onWifiRestored = nil
    }

    private func sendReport(_ report: NetworkReport, handle: UnsafeMutableRawPointer) {
        guard let json = try? JSONEncoder().encode(report),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return }
        let result = jsonStr.withCString { nmp_app_podcast_network_report(handle, $0) }
        if let result { nmp_free_string(result) }
    }
}

// MARK: - Wire type

private struct NetworkReport: Encodable {
    enum CodingKeys: String, CodingKey {
        case type, isWifi = "is_wifi", isConnected = "is_connected"
    }
    let isWifi: Bool
    let isConnected: Bool
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode("connectivity_changed", forKey: .type)
        try c.encode(isWifi, forKey: .isWifi)
        try c.encode(isConnected, forKey: .isConnected)
    }
}
