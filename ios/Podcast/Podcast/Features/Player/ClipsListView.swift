import SwiftUI

// MARK: - ClipsListView
//
// Lists every user-saved clip across all episodes. Source of truth is
// `model.podcastSnapshot?.clips`, which the Rust kernel re-projects with
// fresh episode + podcast titles on every snapshot tick (D7).
//
// Each row:
//   - Podcast name + episode title
//   - `[start–end]` range with clip duration
//   - ShareLink so iOS surfaces the system share sheet
//   - Delete button (swipe action + trailing trash icon)

struct ClipsListView: View {
    @Environment(KernelModel.self) private var model

    private var clips: [ClipSummary] {
        model.podcastSnapshot?.clips ?? []
    }

    var body: some View {
        Group {
            if clips.isEmpty {
                PodcastPlaceholder(
                    systemImage: "scissors",
                    title: "No clips yet",
                    subtitle: "Use the scissors button in the player to capture a moment."
                )
            } else {
                List {
                    ForEach(clips) { clip in
                        ClipsListRow(clip: clip) {
                            delete(clip)
                        }
                    }
                    .onDelete { indexSet in
                        for index in indexSet {
                            delete(clips[index])
                        }
                    }
                }
                .listStyle(.insetGrouped)
            }
        }
        .navigationTitle("Clips")
        .navigationBarTitleDisplayMode(.large)
    }

    private func delete(_ clip: ClipSummary) {
        model.dispatch(namespace: "podcast.clip", body: [
            "op": "delete",
            "clip_id": clip.id,
        ])
    }
}

// MARK: - Row

private struct ClipsListRow: View {
    let clip: ClipSummary
    let onDelete: () -> Void

    private var rangeText: String {
        let length = max(0, clip.endSecs - clip.startSecs)
        return "\(formatDuration(clip.startSecs))–\(formatDuration(clip.endSecs)) · \(formatDuration(length))"
    }

    private var shareText: String {
        let titleLine = clip.title?.isEmpty == false
            ? clip.title!
            : "Clip from \(clip.episodeTitle)"
        return """
        \(titleLine)
        \(clip.podcastTitle) — \(clip.episodeTitle)
        \(rangeText)
        """
    }

    var body: some View {
        VStack(alignment: .leading, spacing: PodcastSpace.xs) {
            if let title = clip.title, !title.isEmpty {
                Text(title)
                    .font(PodcastFont.callout.weight(.semibold))
                    .lineLimit(2)
            }
            Text(clip.episodeTitle)
                .font(PodcastFont.callout)
                .lineLimit(2)
            Text(clip.podcastTitle)
                .font(PodcastFont.caption)
                .foregroundStyle(PodcastColor.textSecondary)
            HStack(spacing: PodcastSpace.m) {
                Text(rangeText)
                    .font(PodcastFont.caption.monospacedDigit())
                    .foregroundStyle(PodcastColor.textSecondary)
                Spacer(minLength: 0)
                ShareLink(item: shareText) {
                    Image(systemName: "square.and.arrow.up")
                        .font(.system(size: 16, weight: .semibold))
                        .frame(width: 32, height: 32)
                }
                .accessibilityLabel("Share clip")
                Button(role: .destructive, action: onDelete) {
                    Image(systemName: "trash")
                        .font(.system(size: 16, weight: .semibold))
                        .frame(width: 32, height: 32)
                        .foregroundStyle(PodcastColor.danger)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Delete clip")
            }
        }
        .padding(.vertical, PodcastSpace.xs)
        .swipeActions(edge: .trailing) {
            Button(role: .destructive, action: onDelete) {
                Label("Delete", systemImage: "trash")
            }
        }
    }

}
