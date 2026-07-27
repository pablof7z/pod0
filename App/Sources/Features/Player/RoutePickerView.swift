import AVKit
import SwiftUI
import UIKit

// MARK: - RoutePickerView

/// Thin SwiftUI wrapper around `AVRoutePickerView` so SwiftUI surfaces can
/// host the system's audio route picker. Tapping presents the system sheet
/// — the OS handles AirPlay / Bluetooth / USB-C selection without us
/// owning any AVAudioSession routing logic.
///
/// Tints are exposed so callers can render the glyph against the
/// surrounding chrome. By default the inner button's icon is a clear
/// AirPlay glyph the OS draws; pass `tintColor: .clear` to suppress it
/// when presenting from a custom control.
struct RoutePickerView: UIViewRepresentable {
    var activeTintColor: UIColor = .tintColor
    var tintColor: UIColor = .label
    var accessibilityName: String? = nil

    func makeUIView(context: Context) -> AVRoutePickerView {
        let view = AVRoutePickerView()
        view.prioritizesVideoDevices = false
        view.backgroundColor = .clear
        configureAccessibility(in: view)
        return view
    }

    func updateUIView(_ uiView: AVRoutePickerView, context: Context) {
        uiView.activeTintColor = activeTintColor
        uiView.tintColor = tintColor
        configureAccessibility(in: uiView)
    }

    private func configureAccessibility(in routePicker: AVRoutePickerView) {
        guard let accessibilityName else { return }
        DispatchQueue.main.async {
            routePicker.firstDescendantButton?.accessibilityLabel = accessibilityName
        }
    }
}

private extension UIView {
    var firstDescendantButton: UIButton? {
        if let button = self as? UIButton { return button }
        return subviews.lazy.compactMap(\.firstDescendantButton).first
    }
}
