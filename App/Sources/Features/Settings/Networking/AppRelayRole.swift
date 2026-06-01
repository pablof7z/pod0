import SwiftUI

/// The canonical NIP-65 role choices the user can assign to an app relay.
///
/// Each case's `wireValue` is the exact role string the Rust kernel emits and
/// accepts. The kernel canonicalises every role through
/// `nmp_core::actor::relay_roles::Nip65Role::to_canonical_string` (verified at
/// NMP rev `7be4a771`): read+write collapses to `both`, and the indexer lane is
/// appended last, so the only composite form the projection can produce is
/// `both,indexer` — never `indexer,both` and never space-joined. Keeping the
/// add/edit pickers keyed to these exact strings is what guarantees a relay the
/// user just created renders with a coloured badge instead of falling through to
/// the raw-string gray case in `AppRelayBadge`.
enum AppRelayRole: String, CaseIterable, Identifiable {
    case read
    case write
    case both
    case indexer
    case bothIndexer = "both,indexer"

    var id: String { rawValue }

    /// The role string sent to / received from the kernel.
    var wireValue: String { rawValue }

    /// Human-facing label for pickers and badges.
    var label: String {
        switch self {
        case .read: return "Read"
        case .write: return "Write"
        case .both: return "Both"
        case .indexer: return "Indexer"
        case .bothIndexer: return "Both + Indexer"
        }
    }

    /// Default role offered when adding a relay (matches the kernel's
    /// `Nip65Role::BOTH` default of read+write).
    static let addDefault: AppRelayRole = .both
}

/// Resolves an arbitrary kernel-emitted role string to its label + colour.
///
/// Known canonical forms map to a coloured pill; any unrecognised string
/// (e.g. a composite the picker can't produce, like `read,indexer`) falls back
/// to gray and renders the raw role verbatim, per the editor spec.
enum AppRelayRoleStyle {
    static func color(for role: String) -> Color {
        switch role {
        case AppRelayRole.indexer.wireValue: return .purple
        case AppRelayRole.bothIndexer.wireValue: return .teal
        case AppRelayRole.both.wireValue: return .green
        case AppRelayRole.read.wireValue: return .blue
        case AppRelayRole.write.wireValue: return .orange
        default: return .gray
        }
    }

    static func label(for role: String) -> String {
        AppRelayRole(rawValue: role)?.label ?? role
    }
}

/// A color-coded pill describing one relay's NIP-65 role.
struct AppRelayBadge: View {
    let role: String

    var body: some View {
        let tint = AppRelayRoleStyle.color(for: role)
        Text(AppRelayRoleStyle.label(for: role))
            .font(AppTheme.Typography.caption)
            .foregroundStyle(tint)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(Capsule().fill(tint.opacity(0.15)))
    }
}
