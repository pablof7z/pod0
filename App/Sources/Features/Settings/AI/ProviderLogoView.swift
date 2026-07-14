import SwiftUI

/// Provider logo that loads the models.dev icon URL when available,
/// falling back to a deterministic letter-tile.
struct ProviderLogoView: View {
    let providerID: String
    let providerName: String
    var iconURL: URL? = nil
    var size: CGFloat = 36

    // MARK: - Layout constants

    private enum Layout {
        /// Padding applied inside the remote icon image, as a fraction of total size.
        static let iconPaddingRatio: CGFloat = 0.12
        /// Monogram font size as a fraction of the tile size.
        static let monogramFontRatio: CGFloat = 0.4
        /// Opacity of the lighter gradient stop in the fallback letter tile.
        static let gradientStartOpacity: Double = 0.9
        /// HSB saturation of the generated tile color.
        static let tileSaturation: Double = 0.6
        /// HSB brightness of the generated tile color.
        static let tileBrightness: Double = 0.75
        /// Divisor for converting a hash-derived hue integer (0–359) into a 0–1 Double.
        static let hueDivisor: Double = 360.0
        /// Modulus used to keep the hash-derived hue value within 0–359.
        static let hueMod: Int = 360
    }

    var body: some View {
        Group {
            if let url = iconURL {
                CachedAsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image
                            .resizable()
                            .scaledToFit()
                            .padding(size * Layout.iconPaddingRatio)
                    default:
                        letterTile
                    }
                }
            } else {
                letterTile
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }

    private var letterTile: some View {
        ZStack {
            Circle()
                .fill(
                    LinearGradient(
                        colors: [tileColor.opacity(Layout.gradientStartOpacity), tileColor],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            Text(monogram)
                .font(.system(size: size * Layout.monogramFontRatio, weight: .bold, design: .rounded))
                .foregroundStyle(.white)
        }
    }

    private var tileColor: Color {
        let hue = abs(providerID.hashValue) % Layout.hueMod
        return Color(hue: Double(hue) / Layout.hueDivisor, saturation: Layout.tileSaturation, brightness: Layout.tileBrightness)
    }

    private var monogram: String {
        let pieces = providerName
            .split(whereSeparator: { $0 == " " || $0 == "-" || $0 == "." })
            .prefix(2)
        let text = pieces.compactMap(\.first).map(String.init).joined()
        return text.isEmpty ? String(providerID.prefix(2)).uppercased() : text.uppercased()
    }
}
