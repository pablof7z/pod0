import SwiftUI

// MARK: - AddFriendSheet

/// Sheet for adding a friend by scanning their Nostr QR code or pasting their public key.
///
/// When `prefillNpub` / `prefillName` are provided (from an invite deep-link), the sheet
/// opens directly in paste mode with those values already filled in.
struct AddFriendSheet: View {
    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    /// Optional npub (full bech32 `npub1…`) to pre-fill from an invite deep-link.
    var prefillNpub: String? = nil
    /// Optional display name to pre-fill from an invite deep-link.
    var prefillName: String? = nil

    @State private var mode: Mode = .camera
    @State private var displayName = ""
    @State private var identifier = ""
    @State private var scanned = false
    @FocusState private var nameFocused: Bool

    private enum Mode: CaseIterable, Hashable {
        case camera
        case paste

        var label: String {
            switch self {
            case .camera: return "Camera"
            case .paste:  return "Paste"
            }
        }
    }

    private enum Layout {
        static let viewfinderSize: CGFloat = 200
        static let viewfinderLineWidth: CGFloat = 2
        static let viewfinderBorderOpacity: Double = 0.6
        static let pillHorizontalPadding: CGFloat = 16
        static let pillVerticalPadding: CGFloat = 8
        static let pillBottomPadding: CGFloat = 32
    }

    private var cleanedIdentifier: String {
        let trimmed = identifier.trimmed
        if trimmed.lowercased().hasPrefix("npub1") {
            return String(trimmed.dropFirst("npub1".count))
        }
        return trimmed
    }

    private var isValid: Bool {
        !displayName.isBlank &&
        cleanedIdentifier.count >= 32
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                modePicker
                if mode == .camera {
                    cameraPanel
                } else {
                    pastePanel
                }
            }
            .navigationTitle("Add Friend")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if mode == .paste {
                        Button("Add") { add() }
                            .fontWeight(.semibold)
                            .disabled(!isValid)
                    }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onAppear { applyPrefillIfNeeded() }
    }

    // MARK: - Prefill

    private var modePicker: some View {
        LiquidGlassSegmentedPicker(
            "Add friend method",
            selection: $mode,
            segments: Mode.allCases.map { ($0, $0.label) }
        )
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.vertical, AppTheme.Spacing.sm)
    }

    private func applyPrefillIfNeeded() {
        guard let npub = prefillNpub, !npub.isEmpty else { return }
        identifier = npub
        if let name = prefillName, !name.isEmpty {
            displayName = name
        }
        mode = .paste
        // Focus the name field when prefilled so the user can confirm or change it.
        Task { @MainActor in nameFocused = true }
    }

    // MARK: - Camera panel

    private var cameraPanel: some View {
        ZStack {
            scannerLayer
            viewfinderFrame
            instructionPill
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var scannerLayer: some View {
        QRCodeScannerView { value in
            guard !scanned else { return }
            scanned = true
            Haptics.success()
            identifier = value
            mode = .paste
            nameFocused = true
        }
        .ignoresSafeArea()
    }

    private var viewfinderFrame: some View {
        VStack {
            Spacer()
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .strokeBorder(.white.opacity(Layout.viewfinderBorderOpacity), lineWidth: Layout.viewfinderLineWidth)
                .frame(width: Layout.viewfinderSize, height: Layout.viewfinderSize)
            Spacer()
        }
    }

    private var instructionPill: some View {
        VStack {
            Spacer()
            Text("Point at a Nostr QR code")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.white)
                .padding(.horizontal, Layout.pillHorizontalPadding)
                .padding(.vertical, Layout.pillVerticalPadding)
                .background(.ultraThinMaterial, in: Capsule())
                .padding(.bottom, Layout.pillBottomPadding)
        }
    }

    // MARK: - Paste panel

    private var pastePanel: some View {
        Form {
            Section {
                TextField("Display name", text: $displayName)
                    .focused($nameFocused)
                    .submitLabel(.next)

                TextField("npub or hex pubkey", text: $identifier)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .font(AppTheme.Typography.monoCallout)
            } footer: {
                Text("Both npub1… and raw hex pubkeys are accepted.")
            }
        }
        .onAppear { if !scanned { nameFocused = true } }
    }

    // MARK: - Actions

    private func add() {
        let name = displayName.trimmed
        guard isValid else { return }
        _ = store.addFriend(displayName: name, identifier: cleanedIdentifier)
        Haptics.success()
        dismiss()
    }
}
