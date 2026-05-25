import AVKit
import SwiftUI
import UIKit

// MARK: - AirPlayRoutePicker
//
// SwiftUI bridge for `AVRoutePickerView`. Renders the standard system AirPlay
// route picker as a SwiftUI view sized to the bound diameter. The tint is
// applied to the contained `UIButton` so the glyph reads on a translucent
// glass surface.
//
// Doctrine: presentation-only. No state, no observers. The picker is a
// system-managed control; tapping it opens the AVKit route sheet directly.

struct AirPlayRoutePicker: UIViewRepresentable {
    var activeTint: UIColor = .systemBlue
    var inactiveTint: UIColor = .label
    var size: CGFloat = 28

    func makeUIView(context: Context) -> AVRoutePickerView {
        let view = AVRoutePickerView(frame: .zero)
        view.activeTintColor = activeTint
        view.tintColor = inactiveTint
        view.prioritizesVideoDevices = false
        view.backgroundColor = .clear
        view.setContentHuggingPriority(.required, for: .horizontal)
        view.setContentHuggingPriority(.required, for: .vertical)
        return view
    }

    func updateUIView(_ view: AVRoutePickerView, context: Context) {
        view.activeTintColor = activeTint
        view.tintColor = inactiveTint
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: AVRoutePickerView, context: Context) -> CGSize? {
        CGSize(width: size, height: size)
    }
}
