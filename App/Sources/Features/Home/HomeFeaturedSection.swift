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
    /// Count of triage decisions currently routed to the inbox (unplayed
    /// only). Drives the subtitle below the section header. Default 0
    /// so legacy callers compile.
    var inboxCount: Int = 0
    /// Count of triage decisions the agent silently archived. Surfaced
    /// in the subtitle as evidence of background work the user didn't
    /// have to see directly.
    var archivedCount: Int = 0
    /// Distinct shows covered by the triage pass. Folded into the
    /// subtitle only when >1 — "across 1 shows" reads as noise.
    var showCount: Int = 0
    /// Wall-clock timestamp of the last completed triage pass. Drives
    /// the "Triaged N ago" prefix; `nil` hides the prefix entirely.
    var lastTriagedAt: Date? = nil
    @Binding var isExpanded: Bool
    let onPlayEpisode: (Episode) -> Void
    let onLongPressEpisode: (Episode) -> Void
    var onOpenThread: () -> Void = { }
    var onSeeAll: () -> Void = { }

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
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: AppTheme.Spacing.sm) {
                    Text(headerTitle)
                        .font(AppTheme.Typography.title3)
                        .foregroundStyle(.primary)
                    if picksBundle.source == .fallback && !picksBundle.picks.isEmpty {
                        // Honest disclosure when the agent fallback heuristic
                        // produced the picks rather than a real LLM call.
                        Text("Heuristic")
                            .font(AppTheme.Typography.caption2)
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, AppTheme.Spacing.xs)
                            .padding(.vertical, 2)
                            .background(Color(.tertiarySystemFill), in: Capsule())
                    }
                    Spacer(minLength: 0)
                    if inboxCount > 0 {
                        Button(action: onSeeAll) {
                            Text("See All")
                                .font(AppTheme.Typography.subheadline)
                                .foregroundStyle(.tint)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("See all inbox episodes")
                    }
                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
                if let subtitle {
                    // Quiet evidence that the agent did autonomous work
                    // since the user last looked.
                    Text(subtitle)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityHeaderLabel)
        // Hints describe the effect — VoiceOver already announces the
        // "double-tap to activate" gesture via the button trait.
        .accessibilityHint(isExpanded ? "Collapses the featured section" : "Expands the featured section")
    }

    private var headerTitle: String {
        // The section previously labelled "Featured" is now the AI Inbox
        // — same hero+secondaries layout, items are persisted autonomous
        // triage decisions instead of ephemeral editorial picks.
        if let name = activeCategoryName, !name.isEmpty {
            return "Inbox · \(name)"
        }
        return "Inbox"
    }

    /// Quietly confident one-liner under the header describing what the
    /// agent did in the background. Returns `nil` when no triage has
    /// run yet — "0 picks · 0 archived" reads as broken.
    private var subtitle: String? {
        guard inboxCount > 0 || archivedCount > 0 else { return nil }
        var middle: String
        if inboxCount > 0 && showCount > 1 {
            middle = "\(inboxCount) \(inboxCount == 1 ? "pick" : "picks") across \(showCount) shows"
        } else if inboxCount > 0 {
            middle = "\(inboxCount) \(inboxCount == 1 ? "pick" : "picks")"
        } else {
            middle = "All caught up"
        }
        var parts: [String] = []
        if let prefix = relativePrefix(for: lastTriagedAt) {
            parts.append(prefix)
        }
        parts.append(middle)
        if archivedCount > 0 {
            parts.append("\(archivedCount) archived")
        }
        return parts.joined(separator: " · ")
    }

    private func relativePrefix(for date: Date?) -> String? {
        guard let date else { return nil }
        let rendered = Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
        return "Triaged \(rendered)"
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    /// Voiced label folds the subtitle in so VoiceOver users hear the
    /// agent's status, not just the title.
    private var accessibilityHeaderLabel: String {
        let stateSuffix = isExpanded ? "expanded" : "collapsed"
        if let subtitle {
            return "\(headerTitle), \(subtitle), \(stateSuffix)"
        }
        return "\(headerTitle), \(stateSuffix)"
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
