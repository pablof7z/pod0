import SwiftUI

// MARK: - BriefingComposeSheet

/// W1 — *Compose* surface from UX-08 §6. Lets the user pick scope, length,
/// and style; freeform "Brief me on…" is optional. On submit, hands a
/// `BriefingRequest` back to the parent through `onCompose`.
struct BriefingComposeSheet: View {

    // MARK: Inputs

    var onCompose: (BriefingRequest) -> Void
    /// Optional prefill for the freeform query field. Used by surfaces
    /// outside Briefings that hand off into compose with a topic context
    /// already in hand (e.g. the Home "Threaded Today" sheet → "Save as
    /// briefing"). Empty by default.
    var initialFreeformQuery: String = ""
    /// Optional prefill for scope. Same handoff path — when a caller knows
    /// the user has a specific topic in mind, we drop them straight onto
    /// the matching scope chip so they can hit Compose immediately.
    var initialScope: BriefingScope?

    // MARK: Local state

    @Environment(\.dismiss) private var dismiss
    @State private var freeformQuery: String = ""
    @State private var length: BriefingLength = .medium
    @State private var scope: BriefingScope = .mySubscriptions
    @State private var style: BriefingStyle = .morning

    // MARK: Body

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                    freeformField
                    lengthSection
                    scopeSection
                    styleSection
                    composeButton
                }
                .padding()
            }
            .background(background)
            .navigationTitle("Compose")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") { dismiss() }
                }
            }
            .onAppear {
                // Apply prefills exactly once on first appearance so a
                // re-presentation doesn't clobber whatever the user edited.
                if freeformQuery.isEmpty {
                    freeformQuery = initialFreeformQuery
                }
                if let initialScope {
                    scope = initialScope
                }
            }
        }
    }

    // MARK: Sections

    private var freeformField: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Brief me on…")
                .font(.title3.weight(.semibold))
            TextField(
                "what's been said about Ozempic",
                text: $freeformQuery,
                axis: .vertical
            )
            .textFieldStyle(.plain)
            .padding(AppTheme.Spacing.md)
            .glassSurface(
                cornerRadius: AppTheme.Corner.lg,
                tint: BriefingsView.brassAmber.opacity(0.10)
            )
        }
    }

    private var lengthSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Length").font(.headline)
            LiquidGlassSegmentedPicker(
                "Length",
                selection: $length,
                segments: BriefingLength.allCases.map { ($0, $0.displayLabel) }
            )
        }
    }

    private var scopeSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Scope").font(.headline)
            HStack {
                ForEach(BriefingScope.allCases, id: \.self) { s in
                    chip(label: scopeLabel(s), isSelected: scope == s) {
                        scope = s
                    }
                }
            }
        }
    }

    private var styleSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Style").font(.headline)
            VStack(spacing: AppTheme.Spacing.xs) {
                ForEach(BriefingStyle.allCases, id: \.self) { s in
                    Button { style = s } label: {
                        HStack {
                            Image(systemName: style == s ? "largecircle.fill.circle" : "circle")
                            Text(s.displayLabel)
                            Spacer()
                        }
                        .padding(AppTheme.Spacing.sm)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var composeButton: some View {
        Button {
            let request = BriefingRequest(
                scope: scope,
                length: length,
                style: style,
                freeformQuery: freeformQuery.isEmpty ? nil : freeformQuery
            )
            onCompose(request)
        } label: {
            Text("Compose Brief")
                .font(.headline)
                .frame(maxWidth: .infinity)
                .padding()
        }
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: BriefingsView.brassAmber.opacity(0.32),
            interactive: true
        )
        .padding(.top, AppTheme.Spacing.md)
    }

    // MARK: Bits

    private func chip(label: String, isSelected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .font(.caption.weight(.medium))
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.xs)
        }
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: isSelected
                ? BriefingsView.brassAmber.opacity(0.30)
                : Color.clear.opacity(0.0),
            interactive: true
        )
        .buttonStyle(.plain)
    }

    private func scopeLabel(_ scope: BriefingScope) -> String {
        switch scope {
        case .mySubscriptions: "My subs"
        case .thisShow:        "This show"
        case .thisTopic:       "This topic"
        case .thisWeek:        "This week"
        }
    }

    private var background: some View {
        LinearGradient(
            colors: [
                BriefingsView.brassAmber.opacity(0.08),
                Color(.systemBackground),
            ],
            startPoint: .top, endPoint: .bottom
        )
        .ignoresSafeArea()
    }
}
