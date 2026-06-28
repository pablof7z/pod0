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
    @State private var validationMessage: String?
    @State private var isAdding = false
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

    private enum FriendInputResolution {
        case success(String)
        case failure(String)
    }

    private var canAttemptAdd: Bool {
        !displayName.isBlank && !identifier.trimmed.isEmpty
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
                        Button(isAdding ? "Adding..." : "Add") { Task { await add() } }
                            .fontWeight(.semibold)
                            .disabled(!canAttemptAdd || isAdding)
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
            validationMessage = nil
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

                TextField("npub, NIP-05, or hex pubkey", text: $identifier)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .font(AppTheme.Typography.monoCallout)
                    .onChange(of: identifier) { _, _ in validationMessage = nil }
            } footer: {
                Text("npub, nprofile, NIP-05 addresses, nostr profile links, and raw hex pubkeys are accepted.")
            }

            if let validationMessage {
                Section {
                    Label(validationMessage, systemImage: "exclamationmark.triangle.fill")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(AppTheme.Tint.error)
                }
            }
        }
        .onAppear { if !scanned { nameFocused = true } }
    }

    // MARK: - Actions

    @MainActor
    private func add() async {
        let name = displayName.trimmed
        guard canAttemptAdd, !isAdding else { return }
        isAdding = true
        validationMessage = nil
        defer { isAdding = false }

        switch await resolveFriendPubkey(from: identifier.trimmed) {
        case .success(let pubkeyHex):
            _ = store.addFriend(displayName: name, identifier: pubkeyHex)
            Haptics.success()
            dismiss()
        case .failure(let message):
            validationMessage = message
            Haptics.warning()
        }
    }

    private func resolveFriendPubkey(from input: String) async -> FriendInputResolution {
        if let envelope = store.classifyNostrDiscoveryIntent(input: input),
           envelope.ok,
           let classification = envelope.classification {
            switch classification {
            case .rejection(.secretLike):
                return .failure("This looks like a Nostr private key. Do not paste private keys here.")
            case .rejection(.unparseable):
                break
            case .rejection:
                return .failure("That Nostr input is not available from Add Friend yet.")
            case .candidates(let candidates):
                guard let target = candidates.first?.target else { break }
                return await resolveFriendPubkey(from: target, originalInput: input)
            }
        }

        if Self.isRawHexPubkey(input) {
            return .success(input.lowercased())
        }
        return .failure("Paste an npub, nprofile, NIP-05 address, nostr profile link, or raw hex pubkey.")
    }

    private func resolveFriendPubkey(
        from target: NostrIntentTarget,
        originalInput: String
    ) async -> FriendInputResolution {
        switch target {
        case .directRef(let uri):
            guard let decoded = store.decodeNostrRef(uri: uri) else {
                return .failure("That Nostr reference could not be decoded.")
            }
            switch decoded {
            case .profile(let pubkey), .address(let pubkey):
                return .success(pubkey)
            case .event:
                return .failure("Nostr event links cannot be added as friends. Paste an npub or nprofile.")
            }
        case .nip05(let identifier):
            return await resolveNip05FriendPubkey(identifier, originalInput: originalInput)
        case .relayURL, .textQuery:
            return .failure("Paste a Nostr public-key reference, not a search query or relay URL.")
        case .registered:
            return .failure("That Nostr input is not supported here yet.")
        }
    }

    private func resolveNip05FriendPubkey(
        _ identifier: String,
        originalInput: String
    ) async -> FriendInputResolution {
        let existingProfiles = store.resolvedNostrProfilePubkeys()
        let outcome = store.dispatchNostrDiscoveryIntent(
            input: originalInput,
            sessionID: "add-friend-\(UUID().uuidString)"
        )
        guard case .dispatched(.nip05(identifier: _)) = outcome else {
            return .failure("That NIP-05 address could not be resolved from Add Friend.")
        }
        guard let pubkey = await store.awaitResolvedNostrProfilePubkey(
            excluding: existingProfiles,
            timeout: .seconds(5)
        ) else {
            return .failure("Could not resolve \(identifier). Try an npub or nprofile.")
        }
        return .success(pubkey)
    }

    private static func isRawHexPubkey(_ value: String) -> Bool {
        let lower = value.lowercased()
        guard lower.count == 64 else { return false }
        let hex = CharacterSet(charactersIn: "0123456789abcdef")
        return lower.unicodeScalars.allSatisfy { hex.contains($0) }
    }
}
