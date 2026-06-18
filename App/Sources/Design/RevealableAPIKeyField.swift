import SwiftUI

/// A text field that toggles between secure and plain entry with an eye button.
///
/// Use this wherever a user pastes an API key and needs the option to reveal it.
///
/// - Parameter accessibilityIdentifier: If set, the inner SecureField and TextField
///   both receive this identifier, enabling deterministic XCTest queries via
///   `app.secureTextFields.matching(identifier:)` (hidden) and
///   `app.textFields.matching(identifier:)` (revealed).
struct RevealableAPIKeyField: View {
    enum IconStyle {
        case outline
        case filled
    }

    let placeholder: String
    @Binding var text: String
    var iconStyle: IconStyle = .outline
    var tint: Color = .secondary
    var isDisabled: Bool = false
    var accessibilityIdentifier: String? = nil

    @State private var isRevealed = false

    init(
        _ placeholder: String,
        text: Binding<String>,
        iconStyle: IconStyle = .outline,
        tint: Color = .secondary,
        isDisabled: Bool = false,
        accessibilityIdentifier: String? = nil
    ) {
        self.placeholder = placeholder
        self._text = text
        self.iconStyle = iconStyle
        self.tint = tint
        self.isDisabled = isDisabled
        self.accessibilityIdentifier = accessibilityIdentifier
    }

    var body: some View {
        HStack {
            if isRevealed {
                TextField(placeholder, text: $text)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .accessibilityIdentifierIfPresent(accessibilityIdentifier)
            } else {
                SecureField(placeholder, text: $text)
                    .accessibilityIdentifierIfPresent(accessibilityIdentifier)
            }
            Button {
                isRevealed.toggle()
            } label: {
                Image(systemName: eyeIconName)
                    .foregroundStyle(tint)
            }
            .buttonStyle(.plain)
            .disabled(isDisabled)
            .accessibilityLabel(isRevealed ? "Hide API key" : "Show API key")
        }
    }

    private var eyeIconName: String {
        switch iconStyle {
        case .outline: isRevealed ? "eye.slash"      : "eye"
        case .filled:  isRevealed ? "eye.slash.fill" : "eye.fill"
        }
    }
}

// MARK: - View+accessibilityIdentifierIfPresent

private extension View {
    /// Applies `.accessibilityIdentifier` only when the value is non-nil,
    /// avoiding an unnecessary ViewModifier wrapper when no identifier is needed.
    @ViewBuilder
    func accessibilityIdentifierIfPresent(_ id: String?) -> some View {
        if let id {
            self.accessibilityIdentifier(id)
        } else {
            self
        }
    }
}
