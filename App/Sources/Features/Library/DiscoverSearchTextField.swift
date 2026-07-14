import SwiftUI
import UIKit

// MARK: - DiscoverSearchTextField

struct DiscoverSearchTextField: UIViewRepresentable {
    let placeholder: String
    @Binding var text: String
    @Binding var isFocused: Bool
    let onSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, isFocused: $isFocused, onSubmit: onSubmit)
    }

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField(frame: .zero)
        field.placeholder = placeholder
        field.autocapitalizationType = .none
        field.autocorrectionType = .no
        field.returnKeyType = .search
        field.backgroundColor = .clear
        field.font = .preferredFont(forTextStyle: .body)
        field.adjustsFontForContentSizeCategory = true
        field.delegate = context.coordinator
        field.addTarget(
            context.coordinator,
            action: #selector(Coordinator.textDidChange(_:)),
            for: .editingChanged
        )
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.setContentHuggingPriority(.required, for: .vertical)
        field.setContentCompressionResistancePriority(.required, for: .vertical)
        return field
    }

    func updateUIView(_ field: UITextField, context: Context) {
        context.coordinator.text = $text
        context.coordinator.isFocused = $isFocused
        context.coordinator.onSubmit = onSubmit
        field.placeholder = placeholder
        if field.text != text {
            field.text = text
        }
        if isFocused, !field.isFirstResponder {
            field.becomeFirstResponder()
        } else if !isFocused, field.isFirstResponder {
            field.resignFirstResponder()
        }
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var text: Binding<String>
        var isFocused: Binding<Bool>
        var onSubmit: () -> Void
        private var keepFocusUntil: Date = .distantPast

        init(text: Binding<String>, isFocused: Binding<Bool>, onSubmit: @escaping () -> Void) {
            self.text = text
            self.isFocused = isFocused
            self.onSubmit = onSubmit
        }

        @objc func textDidChange(_ field: UITextField) {
            keepFocusUntil = Date().addingTimeInterval(1)
            text.wrappedValue = field.text ?? ""
            if !isFocused.wrappedValue {
                isFocused.wrappedValue = true
            }
        }

        func textFieldDidBeginEditing(_ textField: UITextField) {
            isFocused.wrappedValue = true
        }

        func textFieldDidEndEditing(_ textField: UITextField) {
            guard isFocused.wrappedValue else { return }
            guard Date() < keepFocusUntil else {
                isFocused.wrappedValue = false
                return
            }
            DispatchQueue.main.async { [weak self, weak textField] in
                guard let self, self.isFocused.wrappedValue else { return }
                textField?.becomeFirstResponder()
            }
        }

        func textFieldShouldEndEditing(_ textField: UITextField) -> Bool {
            !isFocused.wrappedValue || Date() >= keepFocusUntil
        }

        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            onSubmit()
            return false
        }
    }
}
