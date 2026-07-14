import SwiftUI
import UIKit

// MARK: - PodcastSearchEmptyState

/// Two complementary empty surfaces for `PodcastSearchView`:
///
///   - `prompt` — shown when the search bar is empty. Editorial pitch with
///     three tap-to-execute example queries pulled straight from the spec's
///     "Sample Marquee User Stories" so the user immediately sees what the
///     app can do beyond literal text matching.
///   - `noResults` — shown when a typed query returns zero hits across
///     shows / episodes / transcripts / wiki. Lets the user hand the query
///     off to the conversational agent without retyping.
///
/// Both surfaces use the shared editorial typography (New York serif body)
/// and the existing `glassSurface` modifier so the pills feel like part of
/// the calm-on-the-outside aesthetic the rest of the app commits to.
enum PodcastSearchEmptyState {

    /// Examples lifted from `docs/spec/PROJECT_CONTEXT.md` §"Sample Marquee
    /// User Stories". Kept short enough to fit on one or two lines on the
    /// narrowest supported width without the pill clipping.
    static let exampleQueries: [String] = [
        "Play yesterday's Tim Ferriss where he talked about keto",
        "What was that podcast last week about stamps?",
        "TLDR this week's podcasts in 12 minutes"
    ]
}

// MARK: - PromptEmptyState

/// The "no query yet" surface. Replaces the previous bare scope-pill row
/// with editorial copy + three example queries that route through the
/// caller's `onRunQuery` closure. Tapping a pill writes the example into
/// the search field — `searchable` then drives the rest of the pipeline.
struct PodcastSearchPromptEmptyState: View {
    let onRunQuery: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
            header
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                Text("Try asking")
                    .font(AppTheme.Typography.subheadline.weight(.semibold))
                    .foregroundStyle(.secondary)
                ForEach(PodcastSearchEmptyState.exampleQueries, id: \.self) { example in
                    PodcastSearchExamplePill(text: example) {
                        Haptics.selection()
                        onRunQuery(example)
                    }
                }
            }
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.vertical, AppTheme.Spacing.xl)
        .frame(maxWidth: .infinity, alignment: .leading)
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
        .listRowInsets(EdgeInsets())
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Image(systemName: "sparkle.magnifyingglass")
                .font(.system(size: 36, weight: .light))
                .foregroundStyle(AppTheme.Tint.agentSurface)
                .padding(.bottom, AppTheme.Spacing.xs)
            Text("Search shows, episodes, transcripts.")
                .font(AppTheme.Typography.title)
                .foregroundStyle(.primary)
                .multilineTextAlignment(.leading)
            Text("Or hand the question to the agent.")
                .font(AppTheme.Typography.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.leading)
        }
    }
}

// MARK: - PodcastSearchExamplePill

/// A single tap-to-execute example query. Editorial body type + the shared
/// `glassSurface` capsule so the pill reads as a quotable line rather than
/// a button-shaped affordance — calm by default.
struct PodcastSearchExamplePill: View {
    let text: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "quote.opening")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(AppTheme.Tint.agentSurface.opacity(0.85))
                Text(text)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.primary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: AppTheme.Spacing.xs)
                Image(systemName: "arrow.up.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassSurface(cornerRadius: AppTheme.Corner.lg, interactive: true)
            .contentShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityHint("Run this example query")
    }
}

// MARK: - NoResultsAskAgent

/// "Zero hits" surface. Couples the standard `ContentUnavailableView.search`
/// idiom (so the system surfaces the user's literal query in the title)
/// with a CTA that hands the query off to the agent surface via the
/// `podcastr://agent` deep link — the same path the toolbar's sparkle
/// shortcut uses, so we avoid leaking a tab-binding into the search view.
struct PodcastSearchNoResultsView: View {
    let query: String

    var body: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            ContentUnavailableView.search(text: query)
                .frame(maxWidth: .infinity)

            Button {
                Haptics.selection()
                openAgent()
            } label: {
                HStack(spacing: AppTheme.Spacing.sm) {
                    Image(systemName: "sparkles")
                        .font(.body.weight(.semibold))
                    Text("Ask the agent instead")
                        .font(AppTheme.Typography.body.weight(.semibold))
                }
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.vertical, AppTheme.Spacing.md)
                .frame(maxWidth: .infinity)
                .glassSurface(
                    cornerRadius: AppTheme.Corner.lg,
                    tint: AppTheme.Tint.agentSurface,
                    interactive: true
                )
                .foregroundStyle(.primary)
                .contentShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
            }
            .buttonStyle(.plain)
            .padding(.horizontal, AppTheme.Spacing.lg)
            .accessibilityHint("Switch to the conversational agent and ask your question there")
        }
        .padding(.vertical, AppTheme.Spacing.lg)
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
        .listRowInsets(EdgeInsets())
    }

    /// Routes through the existing `podcastr://agent` deep link rather than
    /// reaching into `RootView`'s tab binding — keeps Search self-contained
    /// and reuses the deep-link handler that already drives the toolbar's
    /// sparkle shortcut.
    private func openAgent() {
        guard let url = URL(string: "podcastr://agent") else { return }
        UIApplication.shared.open(url)
    }
}
