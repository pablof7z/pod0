import SwiftUI

// MARK: - AdvancedView
//
// Per identity-05-synthesis §4.5. Lead paragraph in body / .secondary, hairline
// divider separates sign-in options from account-management options. Sign-in
// options are shown only while the clean identity slot is empty.

struct AdvancedView: View {

    @Environment(UserIdentityStore.self) private var identity
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Form {
            Section {
                Text(introCopy)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.secondary)
                    .listRowBackground(Color.clear)
            }
            if !identity.hasIdentity {
                Section {
                    NavigationLink {
                        UseMyOwnKeyView(onImportComplete: { dismiss() })
                    } label: {
                        advancedRow(
                            title: "Use my own key",
                            subtitle: "Already have an account from another app?",
                            systemImage: "key"
                        )
                    }
                    NavigationLink {
                        RemoteSignerView()
                    } label: {
                        advancedRow(
                            title: "Sign in with a remote signer",
                            subtitle: "Keep your key in a separate signing app.",
                            systemImage: "link.icloud"
                        )
                    }
                }
                Section {
                    Label(
                        "Creating a new account is unavailable until NMP issue #588 provides secure key generation.",
                        systemImage: "exclamationmark.lock"
                    )
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                }
            }
            if identity.hasIdentity {
                Section {
                NavigationLink {
                    AccountDetailsView()
                } label: {
                    advancedRow(
                        title: "Account details",
                        subtitle: "Full account ID, public key formats",
                        systemImage: "doc.text.magnifyingglass"
                    )
                }
                }
            }
        }
        .navigationTitle("Advanced")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Row

    private func advancedRow(
        title: String,
        subtitle: String,
        systemImage: String,
        destructive: Bool = false
    ) -> some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Image(systemName: systemImage)
                .font(AppTheme.Typography.body)
                .foregroundStyle(destructive ? .red : .secondary)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(destructive ? .red : .primary)
                Text(subtitle)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Copy

    private var introCopy: String {
        """
        Most people will never need anything on this page. \
        It's here for people coming from other apps that use the same kind of account.
        """
    }

}
