import SwiftUI

// MARK: - BriefingsView
//
// Top-level Briefings tab. Reads `model.podcastSnapshot?.briefing` and renders
// one of three states:
//
//   1. No briefing slot in the snapshot     → empty state + generate CTA
//   2. `is_generating == true`              → generating placeholder
//   3. `segments` populated                 → list of segment cards
//
// All policy lives in Rust (the kernel decides what `segments` contains,
// what `lastGeneratedAt` is, when `isGenerating` flips). The view is a
// pure projection of the snapshot — tapping the CTA dispatches the
// `podcast.generate_briefing` action and surfaces the in-band envelope
// through the kernel's toast mechanism on failure.
//
// File budget: keep this view under the 300-line soft limit. Segment row
// rendering is split into `BriefingSegmentRow` so the empty / generating
// / list branches stay scannable.

struct BriefingsView: View {
    @Environment(KernelModel.self) private var model

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("Briefings")
                .toolbar { toolbar }
        }
    }

    // MARK: - Branches

    @ViewBuilder
    private var content: some View {
        if let briefing = model.podcastSnapshot?.briefing {
            if briefing.isGenerating && briefing.segments.isEmpty {
                generatingState
            } else if briefing.segments.isEmpty {
                emptyState(briefing: briefing)
            } else {
                segmentList(briefing: briefing)
            }
        } else {
            emptyState(briefing: nil)
        }
    }

    // MARK: - Empty state

    private func emptyState(briefing: BriefingSnapshot?) -> some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            Spacer(minLength: AppTheme.Spacing.xl)

            Image(systemName: "newspaper")
                .font(.system(size: 56))
                .foregroundStyle(.secondary)

            VStack(spacing: AppTheme.Spacing.sm) {
                Text("No briefing yet")
                    .font(.title3.weight(.semibold))
                Text("Generate a daily briefing — an AI-narrated summary of your podcast queue.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, AppTheme.Spacing.xl)
            }

            Button(action: generateBriefing) {
                Label("Generate Daily Briefing", systemImage: "sparkles")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, AppTheme.Spacing.md)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .padding(.horizontal, AppTheme.Spacing.xl)
            .accessibilityIdentifier("briefings-generate-button")

            if let lastGenerated = briefing?.lastGeneratedAt {
                Text("Last briefing: \(relativeDate(unixSecs: lastGenerated))")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - Generating state

    private var generatingState: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            Spacer()
            ProgressView()
                .progressViewStyle(.circular)
                .controlSize(.large)
            VStack(spacing: AppTheme.Spacing.sm) {
                Text("Composing your briefing")
                    .font(.title3.weight(.semibold))
                Text("Selecting episodes and drafting segments…")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, AppTheme.Spacing.xl)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityIdentifier("briefings-generating-state")
    }

    // MARK: - Segment list

    private func segmentList(briefing: BriefingSnapshot) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                if let lastGenerated = briefing.lastGeneratedAt {
                    Text("Generated \(relativeDate(unixSecs: lastGenerated))")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, AppTheme.Spacing.md)
                }

                ForEach(Array(briefing.segments.enumerated()), id: \.offset) { _, segment in
                    BriefingSegmentRow(segment: segment)
                        .padding(.horizontal, AppTheme.Spacing.md)
                }
            }
            .padding(.vertical, AppTheme.Spacing.md)
        }
        .accessibilityIdentifier("briefings-segment-list")
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if let briefing = model.podcastSnapshot?.briefing,
           !briefing.segments.isEmpty,
           !briefing.isGenerating {
            ToolbarItem(placement: .topBarTrailing) {
                Button(action: generateBriefing) {
                    Label("Regenerate", systemImage: "arrow.clockwise")
                }
                .accessibilityIdentifier("briefings-regenerate-button")
            }
        }
    }

    // MARK: - Actions

    private func generateBriefing() {
        model.dispatch(namespace: "podcast", body: ["op": "generate_briefing"])
    }

    // MARK: - Formatting

    private func relativeDate(unixSecs: Int) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(unixSecs))
        return Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .full
        return f
    }()
}
