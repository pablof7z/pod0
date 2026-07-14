import SwiftUI

// MARK: - BriefingBranchPromptSheet

/// Sheet shown when the user taps *↳ deeper* on a briefing player segment.
/// Captures the follow-up prompt that hands off to Lane 6's voice mode via
/// `BriefingPlayerEngine.beginBranch(prompt:)`.
struct BriefingBranchPromptSheet: View {
    @Binding var promptDraft: String
    var onSubmit: () -> Void

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                Text("Tell me more")
                    .font(.title2.weight(.semibold))
                Text("Ask a follow-up — the briefing pauses, the agent answers, then we resume.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                TextField("e.g. headline TPU number?", text: $promptDraft, axis: .vertical)
                    .padding(AppTheme.Spacing.md)
                    .glassSurface(cornerRadius: AppTheme.Corner.lg)
                Spacer()
                Button(action: onSubmit) {
                    Text("Ask")
                        .frame(maxWidth: .infinity)
                        .padding()
                }
                .glassSurface(
                    cornerRadius: AppTheme.Corner.lg,
                    tint: BriefingsView.brassAmber.opacity(0.30),
                    interactive: true
                )
                .buttonStyle(.plain)
            }
            .padding()
        }
        .presentationDetents([.medium])
    }
}
