import SwiftUI

// MARK: - HomeDatelineComponents

/// The four parts that compose the small-caps dateline at the top of Home,
/// surfaced as a value type so the composition rule is testable independent
/// of the SwiftUI render layer.
///
/// Example: `LEARNING · TUESDAY · MAY 5 · 4 NEW · 1 CONTRADICTION`
struct HomeDatelineComponents: Equatable, Sendable {
    /// Optional category-name prefix, uppercased, e.g. `"LEARNING"`. When
    /// non-empty it leads the rendered line so switching categories reads
    /// like flipping to a different magazine section.
    let categoryPrefix: String
    /// Weekday name uppercased — e.g. `"TUESDAY"`.
    let weekday: String
    /// Month + day — e.g. `"MAY 5"`.
    let monthDay: String
    /// Count of unplayed episodes published in the last 24h.
    let newCount: Int
    /// Count of `ThreadingTopic`s with `contradictionCount > 0`.
    let contradictionCount: Int

    /// Joined small-caps line ready for rendering.
    var rendered: String {
        var parts: [String] = []
        if !categoryPrefix.isEmpty {
            parts.append(categoryPrefix)
        }
        parts.append(weekday)
        parts.append(monthDay)
        if newCount > 0 {
            parts.append("\(newCount) NEW")
        }
        if contradictionCount > 0 {
            let label = contradictionCount == 1 ? "CONTRADICTION" : "CONTRADICTIONS"
            parts.append("\(contradictionCount) \(label)")
        }
        return parts.joined(separator: " · ")
    }
}

// MARK: - Pure derivation

enum HomeDateline {

    private static let weekdayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "EEEE"
        return f
    }()

    private static let monthDayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "MMM d"
        return f
    }()

    /// Compose the dateline tokens from the live store + a clock.
    /// Pure function — no environment access — so the tests can pin a
    /// timezone, locale, and "now" without spinning up a SwiftUI view.
    ///
    /// When `categoryName` is non-nil and `allowedSubscriptionIDs` is
    /// supplied, the trailing `NEW` and `CONTRADICTION` counts narrow to
    /// just that category and the rendered line gains an uppercased
    /// category prefix. The contradiction count stays scoped by passing
    /// in only the topics whose mentions land in the category — callers
    /// resolve that upstream because the dateline derivation has no
    /// access to the mention table.
    static func components(
        episodes: [Episode],
        topics: [ThreadingTopic],
        now: Date,
        calendar: Calendar = .current,
        locale: Locale = .current,
        categoryName: String? = nil,
        allowedSubscriptionIDs: Set<UUID>? = nil
    ) -> HomeDatelineComponents {
        let weekdayFmt: DateFormatter
        let monthDayFmt: DateFormatter
        if calendar == .current, locale == .current {
            weekdayFmt = weekdayFormatter
            monthDayFmt = monthDayFormatter
        } else {
            weekdayFmt = DateFormatter()
            weekdayFmt.calendar = calendar
            weekdayFmt.locale = locale
            weekdayFmt.dateFormat = "EEEE"
            monthDayFmt = DateFormatter()
            monthDayFmt.calendar = calendar
            monthDayFmt.locale = locale
            monthDayFmt.dateFormat = "MMM d"
        }
        let weekday = weekdayFmt.string(from: now).uppercased(with: locale)
        let monthDay = monthDayFmt.string(from: now).uppercased(with: locale)
        let prefix = (categoryName ?? "").uppercased(with: locale)

        // Unplayed in the last 24h. We count `!played` episodes whose
        // `pubDate` is within the trailing 24-hour window — the brief asks
        // for "count of unplayed episodes from last 24h" which we read as
        // recent-by-publish-date, not last-fetched.
        let cutoff = now.addingTimeInterval(-86_400)
        let scopedEpisodes = HomeCategoryScope.episodesInCategory(
            episodes,
            allowedSubscriptionIDs: allowedSubscriptionIDs
        )
        let newCount = scopedEpisodes.reduce(0) { acc, ep in
            (!ep.played && !ep.isTriageArchived && ep.pubDate >= cutoff && ep.pubDate <= now) ? acc + 1 : acc
        }

        let contradictionCount = topics.reduce(0) { acc, topic in
            topic.contradictionCount > 0 ? acc + 1 : acc
        }

        return HomeDatelineComponents(
            categoryPrefix: prefix,
            weekday: weekday,
            monthDay: monthDay,
            newCount: newCount,
            contradictionCount: contradictionCount
        )
    }
}

// MARK: - View

/// Small-caps editorial dateline rendered above the Home title. Wraps the
/// pure derivation so the view layer never recomposes the string itself.
struct HomeDatelineView: View {
    let components: HomeDatelineComponents

    var body: some View {
        Text(components.rendered)
            .font(.system(.caption, design: .default).weight(.semibold))
            .tracking(1.6)
            .foregroundStyle(.secondary)
            .accessibilityLabel(accessibilityLabel)
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if !components.categoryPrefix.isEmpty {
            parts.append(components.categoryPrefix)
        }
        parts.append("\(components.weekday), \(components.monthDay)")
        if components.newCount > 0 {
            parts.append("\(components.newCount) new")
        }
        if components.contradictionCount > 0 {
            let label = components.contradictionCount == 1 ? "contradiction" : "contradictions"
            parts.append("\(components.contradictionCount) \(label)")
        }
        return parts.joined(separator: ", ")
    }
}
