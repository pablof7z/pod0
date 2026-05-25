// Compat shim — string / data helpers + UI utilities used by migrated views.
//
// These extensions and helpers live in the legacy `App/Sources/Design/` and
// `App/Sources/Domain/` folders. For M1.E we re-implement the minimal set
// referenced by the migrated Identity / Onboarding / Agent views so they
// compile against the new project without pulling in the full legacy tree.

import Foundation
import SwiftUI
import UIKit

// MARK: - String helpers

extension String {
    /// Returns the string with leading and trailing whitespace + newlines stripped.
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }

    /// True when the trimmed value is empty.
    var isBlank: Bool { trimmed.isEmpty }
}

// MARK: - Data helpers

extension Data {
    /// Hex-decode initializer. Returns nil on odd-length or invalid input.
    init?(hexString: String) {
        let s = hexString.lowercased()
        guard s.count % 2 == 0 else { return nil }
        var bytes: [UInt8] = []
        bytes.reserveCapacity(s.count / 2)
        var index = s.startIndex
        while index < s.endIndex {
            let next = s.index(index, offsetBy: 2)
            guard let byte = UInt8(s[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self = Data(bytes)
    }
}

// MARK: - Bech32 (compat shim)

/// Compat shim — `Bech32.encode` is referenced by `AgentIdentityView` and
/// `NostrNpub.encode`. Real bech32 encoding lands when nmp-keys integration
/// completes; for now returns a placeholder hex preview so the UI renders
/// something deterministic.
enum Bech32 {
    static func encode(hrp: String, data: Data) -> String {
        let hex = data.map { String(format: "%02x", $0) }.joined()
        return "\(hrp)1\(hex)"
    }
}

// MARK: - Haptics

/// Compat shim for the legacy `Haptics` namespace. All entry points are
/// no-ops in the M1.E build to avoid pulling in UIKit feedback generators
/// before the design-system layer ships.
enum Haptics {
    @MainActor static func light() { fire(.light) }
    @MainActor static func medium() { fire(.medium) }
    @MainActor static func selection() {
        UISelectionFeedbackGenerator().selectionChanged()
    }
    @MainActor static func success() { fireNotification(.success) }
    @MainActor static func warning() { fireNotification(.warning) }
    @MainActor static func error() { fireNotification(.error) }

    @MainActor
    private static func fire(_ style: UIImpactFeedbackGenerator.FeedbackStyle) {
        UIImpactFeedbackGenerator(style: style).impactOccurred()
    }

    @MainActor
    private static func fireNotification(_ type: UINotificationFeedbackGenerator.FeedbackType) {
        UINotificationFeedbackGenerator().notificationOccurred(type)
    }
}

// MARK: - Copy to clipboard helper

/// Copies the value onto the pasteboard and flips the bound `isCopied` flag
/// for two seconds so the caller can render a "Copied" badge. Mirrors the
/// legacy helper of the same name.
@MainActor
func copyToClipboard(
    _ value: String,
    isCopied: Binding<Bool>,
    haptic: () -> Void = {}
) {
    UIPasteboard.general.string = value
    isCopied.wrappedValue = true
    haptic()
    Task {
        try? await Task.sleep(for: .seconds(2))
        isCopied.wrappedValue = false
    }
}

// MARK: - Cached async image (compat shim)

/// Compat shim — drops Kingfisher dependency from the legacy implementation.
/// Wraps SwiftUI's stock `AsyncImage` so migrated views compile. Disk + memory
/// caching returns when the Capabilities/ layer gains an HTTP image cache.
struct CachedAsyncImage<Content: View>: View {
    let url: URL?
    let targetSize: CGSize?
    let content: (AsyncImagePhase) -> Content

    init(url: URL?, targetSize: CGSize? = nil, @ViewBuilder content: @escaping (AsyncImagePhase) -> Content) {
        self.url = url
        self.targetSize = targetSize
        self.content = content
    }

    var body: some View {
        AsyncImage(url: url, content: content)
    }
}

// MARK: - Glass surface modifier

extension View {
    /// Compat shim for the legacy `glassSurface` modifier. Renders a
    /// regular-material rounded rectangle with the requested corner radius.
    func glassSurface(cornerRadius: CGFloat = 16, interactive: Bool = false) -> some View {
        self.glassEffect(
            interactive ? .regular.interactive() : .regular,
            in: .rect(cornerRadius: cornerRadius)
        )
    }

    /// Tinted variant — used by `ModeBadge` to colour the capsule by signing mode.
    func glassSurface(cornerRadius: CGFloat = 16, tint: Color, interactive: Bool = false) -> some View {
        self.glassEffect(
            interactive ? .regular.tint(tint).interactive() : .regular.tint(tint),
            in: .rect(cornerRadius: cornerRadius)
        )
    }
}

// MARK: - System share sheet

/// Compat shim — `SystemShareSheet.present(items:)` is called from
/// `AgentIdentityView` for the system activity view controller. Bridges to
/// `UIActivityViewController` against the key window.
@MainActor
enum SystemShareSheet {
    static func present(items: [Any]) {
        guard
            let scene = UIApplication.shared.connectedScenes
                .first(where: { $0.activationState == .foregroundActive }) as? UIWindowScene,
            let root = scene.windows.first(where: { $0.isKeyWindow })?.rootViewController
        else { return }
        let avc = UIActivityViewController(activityItems: items, applicationActivities: nil)
        var presenter: UIViewController = root
        while let next = presenter.presentedViewController { presenter = next }
        presenter.present(avc, animated: true)
    }
}

// MARK: - Keyboard toolbar

/// Adds a "Done" toolbar above the software keyboard that resigns first
/// responder. Mirrors the legacy view extension.
extension View {
    func dismissKeyboardToolbar() -> some View {
        self.toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button("Done") {
                    UIApplication.shared.sendAction(
                        #selector(UIResponder.resignFirstResponder),
                        to: nil, from: nil, for: nil
                    )
                }
                .fontWeight(.semibold)
            }
        }
    }
}

// MARK: - Deep link helper (compat shim)

/// Compat shim — replaced when the deep-link router lands.
enum DeepLinkHandler {
    static func friendInviteURL(npub: String, name: String?) -> URL? {
        var components = URLComponents()
        components.scheme = "podcastr"
        components.host = "friend"
        var queryItems: [URLQueryItem] = [URLQueryItem(name: "npub", value: npub)]
        if let name, !name.isEmpty {
            queryItems.append(URLQueryItem(name: "name", value: name))
        }
        components.queryItems = queryItems
        return components.url
    }
}
