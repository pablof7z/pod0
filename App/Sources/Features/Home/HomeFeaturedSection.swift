import SwiftUI

// MARK: - HomeFeaturedSection

/// The collapsible featured strip at the top of merged Home. Houses both
/// resume cards (in-progress episodes) and agent picks (hero + secondaries).
/// Default expanded with ~280pt height; collapsed state shows just the
/// section header so the user can dismiss the curated surface and get
/// straight to the subscription list below.
///
/// Magazine mode (May 2026): when the user picks a category from the
/// toolbar, every rail in this section narrows to that category — resume
/// rail, agent picks, threaded-today pill — so flipping from "Learning"
/// to "Entertainment" feels like turning to a different magazine section.
/// The content swap is animated via a `.id(categoryID)` on the rail so
/// the new section's shows fade in over the old.
struct HomeFeaturedSection: View {
    let picksBundle: HomeAgentPicksBundle
    /// `true` while the agent service is streaming new picks. Drives the
    /// shimmer placeholder for the *next* pick the model is still emitting
    /// so the user sees the rail filling in incrementally instead of
    /// jumping from empty to fully populated.
    var isStreaming: Bool = false
    /// Top active threading topic. When set, a "Threaded Today" pill renders
    /// below the rail; tapping it invokes `onOpenThread`. `nil` hides the
    /// pill entirely so the section doesn't advertise an empty state.
    var activeThread: ThreadingInferenceService.ActiveTopic? = nil
    /// Active category id (nil = All). Drives the section header label
    /// ("Featured" vs "Featured in Learning") and the `.id`-based rail
    /// content swap that crossfades the shows on category change.
    var activeCategoryID: UUID? = nil
    /// Active category name, surfaced in the header when set.
    var activeCategoryName: String? = nil
    @Binding var isExpanded: Bool
    let onPlayEpisode: (Episode) -> Void
    let onLongPressEpisode: (Episode) -> Void
    var onOpenThread: () -> Void = { }

    @Environment(AppStateStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            header
            if isExpanded {
                // `.id` keyed off the active category so SwiftUI treats the
                // rail as a *new* view when the user switches sections —
                // gives us the cross-fade animation the brief calls for
                // without juggling per-card transitions. `Optional<UUID>`
                // is `Hashable`, so the nil case (All Categories) is its
                // own stable identity without a sentinel uuid.
                rail
                    .id(activeCategoryID)
                    .transition(.opacity.combined(with: .move(edge: .leading)))
                if let active = activeThread {
                    HomeThreadedTodayPill(active: active, onTap: onOpenThread)
                        .padding(.horizontal, AppTheme.Spacing.md)
                        .padding(.top, AppTheme.Spacing.xs)
                        .id(activeCategoryID)
                        .transition(.opacity)
                }
            }
        }
        .animation(AppTheme.Animation.spring, value: activeCategoryID)
    }

    private var header: some View {
        Button {
            Haptics.selection()
            withAnimation(AppTheme.Animation.springFast) {
                isExpanded.toggle()
            }
        } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                Text(headerTitle)
                    .font(AppTheme.Typography.title3)
                    .foregroundStyle(.primary)
                if picksBundle.source == .fallback && !picksBundle.picks.isEmpty {
                    // Honest disclosure when the agent fallback heuristic
                    // produced the picks rather than a real LLM call. The
                    // user shouldn't believe the agent picked these if it
                    // didn't.
                    Text("Heuristic")
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, AppTheme.Spacing.xs)
                        .padding(.vertical, 2)
                        .background(Color(.tertiarySystemFill), in: Capsule())
                }
                Spacer(minLength: 0)
                Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isExpanded ? "\(headerTitle), expanded" : "\(headerTitle), collapsed")
        // Hints describe the effect — VoiceOver already announces the
        // "double-tap to activate" gesture via the button trait.
        .accessibilityHint(isExpanded ? "Collapses the featured section" : "Expands the featured section")
    }

    private var headerTitle: String {
        if let name = activeCategoryName, !name.isEmpty {
            return "Featured in \(name)"
        }
        return "Featured"
    }

    @ViewBuilder
    private var rail: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
                if let hero = picksBundle.hero,
                   let episode = store.episode(id: hero.episodeID) {
                    HomeAgentPickCard(
                        pick: hero,
                        episode: episode,
                        podcast: store.podcast(id: episode.podcastID),
                        isHero: true,
                        onPlay: { onPlayEpisode(episode) }
                    )
                    .onLongPressGesture(minimumDuration: 0.6) {
                        Haptics.medium()
                        onLongPressEpisode(episode)
                    }
                }
                ForEach(picksBundle.secondaries) { pick in
                    if let episode = store.episode(id: pick.episodeID) {
                        HomeAgentPickCard(
                            pick: pick,
                            episode: episode,
                            podcast: store.podcast(id: episode.podcastID),
                            isHero: false,
                            onPlay: { onPlayEpisode(episode) }
                        )
                        .onLongPressGesture(minimumDuration: 0.6) {
                            Haptics.medium()
                            onLongPressEpisode(episode)
                        }
                        .transition(.asymmetric(
                            insertion: .opacity.combined(with: .scale(scale: 0.96)),
                            removal: .opacity
                        ))
                    }
                }
                if isStreaming {
                    // Shimmer slot for the next pick still arriving from the
                    // model. We don't know yet whether it'll be a hero or a
                    // secondary, so the placeholder mirrors the secondary
                    // dimensions — the hero (if any) has already streamed
                    // first per the system prompt's ordering rule.
                    HomeAgentPickShimmerCard(isHero: picksBundle.hero == nil)
                        .transition(.opacity)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .animation(AppTheme.Animation.springFast, value: picksBundle.picks.count)
        }
        // Bound height so the rail doesn't push the subscription list off-
        // screen on small phones. The ~280pt budget the brief calls for
        // accommodates resume + pick cards plus their captions.
        .frame(maxHeight: 320)
    }
}
