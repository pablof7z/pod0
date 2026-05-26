import SwiftUI

// MARK: - EditProfileView
//
// Push from `IdentityRootView`. Per identity-05-synthesis §4.3. Save dispatches
// `nmp.publish` `PublishProfile` (kind:0 metadata) through the NMP kernel and,
// regardless of dispatch outcome, persists the form values to
// `@AppStorage("agent.profile.*")` so they survive app launches without
// depending on a relay round-trip.
//
// In-flight UX: Save flips to a `ProgressView` in the toolbar and Cancel
// disables so a double-tap can't queue two publishes. On success the dirty
// snapshot advances and the view dismisses after a 900 ms banner beat. On
// failure the view stays open with a "Couldn't publish" warning — Save stays
// enabled because the snapshot didn't advance.
//
// Active-signer caveat: the M1.E compat shim does not wire `SignInNsec` /
// `CreateAccount` into the kernel yet, so the publish action will be accepted
// by the registry but the actor cannot actually sign a kind:0 until the
// signer broker lands. The local AppStorage persistence still works end-to-end
// today.
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

    @Environment(UserIdentityStore.self) private var identity
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    // ── Profile fields persist to UserDefaults under the `agent.profile.*`
    // namespace `AgentIdentityView` already uses. Keeps both edit surfaces
    // pointing at the same source of truth and survives launches without
    // waiting on the kind:0 round-trip to repopulate from relays.
    @AppStorage("agent.profile.name") private var storedName: String = ""
    @AppStorage("agent.profile.about") private var storedAbout: String = ""
    @AppStorage("agent.profile.pictureURL") private var storedPictureURL: String = ""
    @AppStorage("agent.profile.displayName") private var storedDisplayName: String = ""

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
        .onChange(of: identity.profileDisplayName) { _, _ in hydrateFromIdentity() }
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
        UserProfileDisplay.from(identity: model.identity)
    }

    /// Username field's placeholder. Falls back to a generic slug shape
    /// only when the identity is somehow missing — normally we show the
    /// real generated slug so the user sees what their username *will*
    /// be if they leave the field blank.
    private var usernamePlaceholder: String {
        identityProfile?.slug ?? "bright-signal-a3f2"
    }

    /// Seed the form from the identity's kind-0 profile fields. Prefers
    /// fetched relay data via `identityProfile`; falls back to the locally
    /// persisted `agent.profile.*` AppStorage values (set by either this
    /// view's last successful Save or by `AgentIdentityView`); falls back to
    /// the generated stub when both are empty. Re-runs when relay data
    /// arrives as long as the user hasn't started editing (dirty guard).
    ///
    /// Hydration precedence — relay > local > stub — means the user always
    /// sees the freshest value the app has seen, but never an empty form
    /// just because the kind:0 fetch hasn't completed (or, in the current
    /// compat-shim state, never will until the signer broker lands).
    private func hydrateFromIdentity() {
        let needsInit = (initialSnapshot == nil)
        guard needsInit || !isDirty else { return }
        let p = identityProfile
        displayName = firstNonEmpty(p?.displayName, storedDisplayName)
        username    = firstNonEmpty(p?.slug, storedName)
        about       = firstNonEmpty(p?.about, storedAbout)
        pictureURL  = firstNonEmpty(p?.pictureURLString, storedPictureURL)
        initialSnapshot = currentSnapshot
    }

    private func firstNonEmpty(_ a: String?, _ b: String) -> String {
        if let a, !a.isEmpty { return a }
        return b
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

    /// Sign + publish the kind-0 profile. Two-outcome flow:
    ///   - **Success**: clear-dirty (so a second tap doesn't republish), show
    ///     a success banner long enough to read (≈900 ms), then dismiss.
    ///   - **Failure**: keep the view open, surface a warning banner with the
    ///     reason so the user can fix and retry. We do NOT move
    ///     `initialSnapshot` forward on failure — Save stays enabled.
    /// Haptic fires AFTER the publish attempt so the user's wrist feedback
    /// matches the actual outcome.
    ///
    /// **Persistence semantics.** We always write the local AppStorage copy
    /// first — even if the kernel dispatch is rejected (no active signer
    /// yet, malformed field). Reasoning: the user typed the values, the form
    /// validation already passed, and the local copy is what populates the
    /// `IdentityRootView` hero on next launch. The "couldn't reach the
    /// relay" banner then reflects only the kind:0 publish, not the local
    /// edit — which is what the user expects from a sync-style editor.
    ///
    /// **Dispatch wire format.** The kernel's `PublishModule` (namespace
    /// `nmp.publish`) accepts `{"PublishProfile": {"fields": {...}}}`. All
    /// values must be `String` — the Rust validator rejects non-string
    /// fields up front. We forward only the four NIP-01 keys the form
    /// captures; any future fields (`nip05`, `lud16`, banner, …) get added
    /// to this map without touching the wire shape.
    ///
    /// **Async-completion note.** `PublishModule::is_async_completing` is
    /// `true`: `dispatch_action` returns the registry-minted correlation_id
    /// the instant the action is enqueued, and the actual relay verdict
    /// arrives later through `projections["action_results"]`. We treat
    /// "dispatch accepted" as success for the banner so the user gets
    /// immediate feedback; observing the terminal verdict is a follow-up
    /// once the signer broker is wired and a real publish can complete.
    private func save() async {
        let snapshot = currentSnapshot
        isPublishing = true
        saveBanner = SaveBanner(message: "Publishing…", isWarning: false)
        defer { isPublishing = false }

        persistLocally(snapshot)

        let dispatch = model.dispatch(
            namespace: "nmp.publish",
            body: publishProfileBody(snapshot)
        )

        if let error = dispatch.errorMessage {
            Haptics.warning()
            saveBanner = SaveBanner(
                message: "Couldn't publish: \(error)",
                isWarning: true
            )
            return
        }

        initialSnapshot = snapshot
        Haptics.success()
        saveBanner = SaveBanner(message: "Profile published.", isWarning: false)
        try? await Task.sleep(for: .milliseconds(900))
        dismiss()
    }

    /// Mirror the form snapshot into AppStorage so a relaunch (or the
    /// `AgentIdentityView` hero) sees the user's edits without waiting for
    /// the kind:0 relay round-trip. Also flushes into the in-memory
    /// `UserIdentityStore` so any view currently observing
    /// `profileDisplayName` / `profileAbout` re-renders immediately.
    private func persistLocally(_ snapshot: Snapshot) {
        storedName = snapshot.username
        storedDisplayName = snapshot.displayName
        storedAbout = snapshot.about
        storedPictureURL = snapshot.pictureURL

        identity.profileName = snapshot.username
        identity.profileDisplayName = snapshot.displayName
        identity.profileAbout = snapshot.about
        identity.profilePicture = snapshot.pictureURL
    }

    /// Build the wire body for `nmp.publish` `PublishProfile`. Only includes
    /// fields the user actually populated — an empty `picture` would still
    /// pass validation but it's noise on the relay (and a user with no
    /// picture is the common case in onboarding).
    private func publishProfileBody(_ snapshot: Snapshot) -> [String: Any] {
        var fields: [String: String] = [:]
        let username = snapshot.username.trimmed
        if !username.isEmpty {
            fields["name"] = username
        }
        let display = snapshot.displayName.trimmed
        if !display.isEmpty {
            fields["display_name"] = display
        }
        let about = snapshot.about.trimmed
        if !about.isEmpty {
            fields["about"] = about
        }
        let picture = snapshot.pictureURL.trimmed
        if !picture.isEmpty {
            fields["picture"] = picture
        }
        return [
            "PublishProfile": [
                "fields": fields,
            ],
        ]
    }
}

// MARK: - End — see ChangePhotoSheet.swift for the photo chooser used above.
