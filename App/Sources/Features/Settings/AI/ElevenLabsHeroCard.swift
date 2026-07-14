import SwiftUI

// MARK: - Connection state

enum ElevenLabsConnectionState {
    case notConnected
    case connectedBYOK
    case connectedManual
    case reconnectRequired

    static func derive(source: ElevenLabsCredentialSource, hasKey: Bool) -> Self {
        switch (source, hasKey) {
        case (.none, _):              return .notConnected
        case (.byok, true):           return .connectedBYOK
        case (.manual, true):         return .connectedManual
        case (.byok, false):          return .reconnectRequired
        case (.manual, false):        return .reconnectRequired
        }
    }
}

// MARK: - Hero card

struct ElevenLabsHeroCard: View {
    let connectionState: ElevenLabsConnectionState
    let keyLabel: String?
    let connectedAt: Date?

    private enum Constants {
        /// Horizontal gap between the icon and the text stack.
        static let hStackSpacing: CGFloat = 14
        /// Font size for the connection icon.
        static let iconSize: CGFloat = 36
        /// Fixed width reserved for the icon so text always aligns.
        static let iconFrameWidth: CGFloat = 44
        /// Vertical gap between title and subtitle in the text stack.
        static let textStackSpacing: CGFloat = 4
        /// Card padding on all sides.
        static let cardPadding: CGFloat = 16
        /// Minimum card height to keep it visually substantial.
        static let cardMinHeight: CGFloat = 88
        /// Corner radius of the glass card surface.
        static let cardCornerRadius: CGFloat = 16
        /// Diameter of the status dot inside the pill.
        static let pillDotSize: CGFloat = 6
        /// Gap between status dot and pill label text.
        static let pillHStackSpacing: CGFloat = 6
        /// Horizontal inset for the status pill capsule.
        static let pillHPadding: CGFloat = 10
        /// Vertical inset for the status pill capsule.
        static let pillVPadding: CGFloat = 5
    }

    var body: some View {
        HStack(spacing: Constants.hStackSpacing) {
            Image(systemName: connectionIcon)
                .font(.system(size: Constants.iconSize, weight: .medium))
                .foregroundStyle(connectionTint)
                .symbolEffect(.bounce, value: connectionState == .notConnected ? 0 : 1)
                .frame(width: Constants.iconFrameWidth)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: Constants.textStackSpacing) {
                Text("ElevenLabs")
                    .font(AppTheme.Typography.title)
                Text(heroSubtitle)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(isConnected ? .primary : .secondary)
                if let tertiary = heroTertiary {
                    Text(tertiary)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()

            if connectionState != .notConnected {
                statusPill
            }
        }
        .padding(Constants.cardPadding)
        .frame(minHeight: Constants.cardMinHeight)
        .glassSurface(cornerRadius: Constants.cardCornerRadius, interactive: true)
    }

    // MARK: - Status pill

    private var statusPill: some View {
        HStack(spacing: Constants.pillHStackSpacing) {
            Circle()
                .fill(pillTint)
                .frame(width: Constants.pillDotSize, height: Constants.pillDotSize)
            Text(pillLabel)
                .font(AppTheme.Typography.caption)
        }
        .padding(.horizontal, Constants.pillHPadding)
        .padding(.vertical, Constants.pillVPadding)
        .glassEffect(.regular.tint(pillTint), in: .capsule)
    }

    // MARK: - Derived values

    private var isConnected: Bool {
        connectionState == .connectedBYOK || connectionState == .connectedManual
    }

    private var connectionIcon: String {
        switch connectionState {
        case .notConnected:      return "waveform.circle"
        case .connectedBYOK:     return "waveform.circle.fill"
        case .connectedManual:   return "waveform.circle.fill"
        case .reconnectRequired: return "exclamationmark.triangle.fill"
        }
    }

    private var connectionTint: Color {
        switch connectionState {
        case .notConnected:      return .secondary
        case .connectedBYOK:     return AppTheme.Brand.elevenLabsTint
        case .connectedManual:   return AppTheme.Brand.elevenLabsTint
        case .reconnectRequired: return .orange
        }
    }

    private var heroSubtitle: String {
        switch connectionState {
        case .notConnected:      return "Not connected"
        case .connectedBYOK:     return "Connected with BYOK"
        case .connectedManual:   return "Manual key saved"
        case .reconnectRequired: return "Reconnect required"
        }
    }

    private var heroTertiary: String? {
        guard isConnected else { return nil }
        if let label = keyLabel, !label.isEmpty { return label }
        if let date = connectedAt {
            return date.formatted(.relative(presentation: .named))
        }
        return nil
    }

    private var pillTint: Color {
        connectionState == .reconnectRequired ? .orange : AppTheme.Brand.elevenLabsTint
    }

    private var pillLabel: String {
        connectionState == .reconnectRequired ? "Action needed" : "Live"
    }
}
