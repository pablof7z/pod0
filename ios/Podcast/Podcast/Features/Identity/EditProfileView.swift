import SwiftUI

// MARK: - EditProfileView
//
// Push from `IdentityRootView`. Per identity-05-synthesis §4.3. Save will
// sign and publish a kind-0 profile event via the kernel's
// `identity.publish_profile` action once that lands (M1 exit). For now the
// in-flight UX is preserved end-to-end but the Save call short-circuits
// to the staged-action banner — no kernel dispatch happens.
//
// Original in-flight UX (still intact for when the action wires up):
// Save flips to a `ProgressView` in the toolbar and Cancel disables so a
// double-tap can't queue two publishes. On success the dirty snapshot
// advances and the view dismisses after a 900 ms banner beat. On failure
// the view stays open with a "Tap Save to retry" warning — Save stays
// enabled because the snapshot didn't advance.
//
// Field rules from §4.3:
//   - Display name: 0-48 chars, empty allowed (falls back to slug)
//   - Username:     1-32 chars (unicode allowed)
//   - About:        0-280 chars; counter visible only when remaining ≤ 50
//   - Save disabled until dirty; .alert on cancel-with-dirty
//   - Errors: inline footer banner, never alert (except cancel-discard)

struct EditProfileView: View {

    private enum Limits {
        static let displayNameMax = 48
        static let usernameMax = 32
        static let usernameMin = 1
        static let aboutMax = 280
        static let aboutCounterThreshold = 50
    }

    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    private var identity: IdentityViewModel { model.identity }

    @State private var displayName: String = ""
    @State private var username: String = ""
    @State private var about: String = ""
    @State private var pictureURL: String = ""
    @State private var pictureSheetPresented = false
    @State private var discardConfirmPresented = false
    @State private var saveBanner: SaveBanner?
    /// True while `publishProfile` is in flight. Drives the toolbar
    /// spinner + Save-button disable so a double-tap can't queue two
    /// publishes for the same edit.
    @State private var isPublishing = false

    /// Captures the initial values so dirty-detection can compare and
    /// cancel-with-dirty can offer a Discard alert.
    @State private var initialSnapshot: Snapshot?

    @FocusState private var nameFocused: Bool
    @FocusState private var usernameFocused: Bool
    @FocusState private var aboutFocused: Bool

    var body: some View {
        Form {
            Section {
                heroRow
            }
            Section("Display name") {
                TextField("e.g. Bright Signal", text: $displayName)
                    .textInputAutocapitalization(.words)
                    .focused($nameFocused)
                    .onChange(of: displayName) { _, new in
                        if new.count > Limits.displayNameMax {
                            displayName = String(new.prefix(Limits.displayNameMax))
                        }
                    }
            }
            Section {
                TextField(usernamePlaceholder, text: $username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($usernameFocused)
                    .onSubmit { restoreUsernameIfBlank() }
                    .onChange(of: username) { _, new in
                        if new.count > Limits.usernameMax {
                            username = String(new.prefix(Limits.usernameMax))
                        }
                    }
            } header: {
                Text("Username")
            } footer: {
                Text("Used to sign your contributions. Letters, numbers, and dashes work best.")
            }
            aboutSection
            if let banner = saveBanner {
                Section { savedBannerView(banner) }
            }
        }
        .navigationTitle("Edit Profile")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                if isPublishing {
                    ProgressView().controlSize(.small)
                } else {
                    Button("Save") { Task { await save() } }
                        .disabled(!isDirty)
                }
            }
            ToolbarItem(placement: .topBarLeading) {
                Button("Cancel") { handleCancel() }
                    .disabled(isPublishing)
            }
        }
        .alert("Discard changes?", isPresented: $discardConfirmPresented) {
            Button("Keep editing", role: .cancel) {}
            Button("Discard", role: .destructive) { dismiss() }
        }
        .sheet(isPresented: $pictureSheetPresented) {
            ChangePhotoSheet(pictureURL: $pictureURL)
        }
        .onAppear { hydrateFromIdentity() }
        // Rehydrate when the kernel's active-account projection changes —
        // covers the kind-0 fetch arriving after the view first appeared
        // as well as the user switching identities.
        .onChange(of: identity.displayName) { _, _ in hydrateFromIdentity() }
        .onChange(of: identity.pictureURLString) { _, _ in hydrateFromIdentity() }
    }

    // MARK: - Hero

    private var heroRow: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            IdentityAvatarView(
                url: previewURL,
                initial: displayName.first ?? identityProfile?.displayName.first,
                size: 88
            )
            Button {
                pictureSheetPresented = true
            } label: {
                Text("Change photo")
                    .font(AppTheme.Typography.caption)
                    .padding(.horizontal, AppTheme.Spacing.md)
                    .padding(.vertical, 6)
            }
            .buttonStyle(.glass)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, AppTheme.Spacing.sm)
    }

    private var previewURL: URL? {
        if let url = URL(string: pictureURL.trimmed),
           let scheme = url.scheme?.lowercased(),
           scheme == "http" || scheme == "https" {
            return url
        }
        return identityProfile?.pictureURL
    }

    // MARK: - About section

    @ViewBuilder
    private var aboutSection: some View {
        Section {
            ZStack(alignment: .topLeading) {
                if about.isEmpty {
                    Text("Tell people who you are.")
                        .foregroundStyle(.tertiary)
                        .padding(.vertical, 8)
                        .padding(.leading, 4)
                        .allowsHitTesting(false)
                }
                TextEditor(text: $about)
                    .frame(minHeight: 80)
                    .focused($aboutFocused)
                    .onChange(of: about) { _, new in
                        if new.count > Limits.aboutMax {
                            about = String(new.prefix(Limits.aboutMax))
                        }
                    }
            }
        } header: {
            Text("About")
        } footer: {
            HStack {
                Spacer()
                if Limits.aboutMax - about.count <= Limits.aboutCounterThreshold {
                    Text("\(Limits.aboutMax - about.count) characters left")
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    // MARK: - Saved banner

    private struct SaveBanner: Equatable {
        let message: String
        let isWarning: Bool
    }

    @ViewBuilder
    private func savedBannerView(_ banner: SaveBanner) -> some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: banner.isWarning ? "wifi.exclamationmark" : "checkmark.circle.fill")
                .foregroundStyle(banner.isWarning ? .orange : .green)
            Text(banner.message)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Snapshot / dirty / hydrate

    private struct Snapshot: Equatable {
        var displayName: String
        var username: String
        var about: String
        var pictureURL: String
    }

    private var currentSnapshot: Snapshot {
        Snapshot(
            displayName: displayName,
            username: username,
            about: about,
            pictureURL: pictureURL
        )
    }

    private var isDirty: Bool {
        guard let initial = initialSnapshot else { return false }
        return initial != currentSnapshot
    }

    private var identityProfile: UserProfileDisplay? {
        UserProfileDisplay.from(identity: identity)
    }

    /// Username field's placeholder. Falls back to a generic slug shape
    /// only when the identity is somehow missing — normally we show the
    /// real generated slug so the user sees what their username *will*
    /// be if they leave the field blank.
    private var usernamePlaceholder: String {
        identityProfile?.slug ?? "bright-signal-a3f2"
    }

    /// Seed the form from the identity's kind-0 profile fields. Prefers
    /// fetched relay data via `identityProfile`; falls back to the generated
    /// stub while the fetch is in flight. Re-runs when relay data arrives as
    /// long as the user hasn't started editing (dirty guard).
    private func hydrateFromIdentity() {
        let needsInit = (initialSnapshot == nil)
        guard needsInit || !isDirty else { return }
        let p = identityProfile
        displayName = p?.displayName ?? ""
        username    = p?.slug ?? ""
        about       = p?.about ?? ""
        pictureURL  = p?.pictureURLString ?? ""
        initialSnapshot = currentSnapshot
    }

    private func restoreUsernameIfBlank() {
        if username.isBlank, let prior = initialSnapshot?.username, !prior.isEmpty {
            username = prior
            Haptics.light()
        }
    }

    // MARK: - Actions

    private func handleCancel() {
        if isDirty {
            discardConfirmPresented = true
        } else {
            dismiss()
        }
    }

    /// Stage the kind-0 publish. The kernel's `identity.publish_profile`
    /// action hasn't shipped yet, so this preserves the original two-
    /// outcome flow's visible UX (in-flight spinner → banner) but always
    /// resolves to the warning banner with the staged-action copy. The
    /// `initialSnapshot` does NOT advance, so Save stays enabled — exactly
    /// the legacy stub's behaviour, just with stable copy.
    ///
    /// When the kernel action lands, this becomes a real `model.dispatch(
    /// namespace: "identity", body: ["op": "publish_profile", …])` call.
    private func save() async {
        isPublishing = true
        saveBanner = SaveBanner(message: "Publishing…", isWarning: false)
        defer { isPublishing = false }
        // Stage-and-toast: route through `KernelModel` so the staged
        // banner shows up via the same channel as real dispatch failures.
        model.surfaceStagedIdentityAction("identity.publish_profile")
        // Hold the "publishing" beat briefly so the spinner is visible.
        try? await Task.sleep(for: .milliseconds(400))
        Haptics.warning()
        saveBanner = SaveBanner(
            message: IdentityViewModel.stagedActionToast,
            isWarning: true
        )
    }
}

// MARK: - End — see ChangePhotoSheet.swift for the photo chooser used above.
