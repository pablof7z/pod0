import SwiftUI

// MARK: - ClipShareSheet
//
// Modal surface presented after the composer commits a `Clip`. Hosts:
//   - A live preview of the image card (re-renders when the user toggles
//     subtitle style).
//   - Three primary share actions: image (PNG), video (MP4), link (URL).
//   - Segmented controls for subtitle style + video aspect ratio.
//
// Each action produces a temp file via `ClipExporter`, then hands the URL
// to a `ShareLink`. We use `ShareLink` (not `UIActivityViewController`) for
// file-URL items per the bug history note in `ShareSheet.swift`.
struct ClipShareSheet: View {
    let clip: Clip
    let episode: Episode
    let podcast: Podcast

    @State private var style: ClipExporter.SubtitleStyle = .editorial
    @State private var aspect: ClipVideo.Aspect = .square

    @State private var imageURL: URL?
    @State private var isRenderingImage = false
    @State private var audioURL: URL?
    @State private var isRenderingAudio = false
    @State private var lastError: String?

    private var deepLink: URL { ClipExporter.shared.deepLink(clip) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: AppTheme.Spacing.lg) {
                    preview
                    styleSection
                    aspectSection
                    actionGrid
                    if let lastError {
                        Text(lastError)
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .multilineTextAlignment(.center)
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.lg)
            }
            .navigationTitle("Share clip")
            .navigationBarTitleDisplayMode(.inline)
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
        }
    }

    // MARK: - Preview

    private var preview: some View {
        ClipImageCardView(
            showName: podcast.title,
            episodeTitle: episode.title,
            artwork: nil,  // Live preview skips network fetch — exporter loads.
            pullQuote: clip.transcriptText,
            speakerName: clip.speakerID,
            timestamp: ClipExporter.formatTimestamp(seconds: clip.startSeconds),
            deepLink: deepLink.absoluteString,
            style: style
        )
        .scaleEffect(0.3)
        .frame(width: 1080 * 0.3, height: 1080 * 0.3)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        .shadow(color: Color.black.opacity(0.12), radius: 24, y: 8)
    }

    // MARK: - Controls

    private var styleSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Subtitle style")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
            LiquidGlassSegmentedPicker(
                "Subtitle style",
                selection: $style,
                segments: ClipExporter.SubtitleStyle.allCases.map { ($0, $0.displayName) }
            )
        }
    }

    private var aspectSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Video aspect ratio")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
            LiquidGlassSegmentedPicker(
                "Aspect",
                selection: $aspect,
                segments: ClipVideo.Aspect.allCases.map { ($0, $0.displayName) }
            )
        }
    }

    // MARK: - Actions

    private var actionGrid: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            imageAction
            audioAction
            videoAction
            linkAction
        }
    }

    private var imageAction: some View {
        Group {
            if let imageURL {
                ShareLink(item: imageURL) {
                    actionRow(systemImage: "photo", title: "Share image",
                              subtitle: "1080×1080 PNG", trailing: "Ready")
                }
            } else {
                Button {
                    Task { await renderImage() }
                } label: {
                    actionRow(systemImage: "photo", title: "Render image",
                              subtitle: "1080×1080 PNG",
                              trailing: isRenderingImage ? "…" : "Tap")
                }
                .disabled(isRenderingImage)
            }
        }
        .onChange(of: style) { _, _ in
            // Style changes invalidate the rendered file.
            imageURL = nil
        }
    }

    /// Third fidelity: trimmed `.m4a` of the source audio. Tap renders +
    /// caches; subsequent taps share the cached URL until the user
    /// dismisses the sheet (temp files self-purge with iOS).
    private var audioAction: some View {
        Group {
            if let audioURL {
                ShareLink(item: audioURL) {
                    actionRow(systemImage: "waveform.circle", title: "Share audio",
                              subtitle: durationSubtitle, trailing: "Ready")
                }
            } else {
                Button {
                    Task { await renderAudio() }
                } label: {
                    actionRow(systemImage: "waveform.circle", title: "Render audio",
                              subtitle: durationSubtitle,
                              trailing: isRenderingAudio ? "…" : "Tap")
                }
                .disabled(isRenderingAudio)
            }
        }
    }

    /// Caption shown under the audio-share row — duration in `M:SS` plus
    /// the m4a tag so the user knows what file type they'll be sharing.
    private var durationSubtitle: String {
        let total = Int(clip.durationSeconds.rounded())
        let m = total / 60
        let s = total % 60
        return String(format: "%d:%02d · M4A", m, s)
    }

    private var videoAction: some View {
        // Video export is intentionally stubbed in this build (see
        // `ClipVideoComposer` header for the punt details). Surface as
        // disabled "Coming soon" rather than letting taps surface a raw
        // `notImplemented` error in the user's face.
        actionRow(
            systemImage: "waveform",
            title: "Share video",
            subtitle: "MP4 with subtitles",
            trailing: "Soon"
        )
        .opacity(0.55)
    }

    private var linkAction: some View {
        ShareLink(item: deepLink) {
            actionRow(
                systemImage: "link",
                title: "Share link",
                subtitle: deepLink.absoluteString,
                trailing: "Ready"
            )
        }
    }

    @ViewBuilder
    private func actionRow(
        systemImage: String,
        title: String,
        subtitle: String,
        trailing: String
    ) -> some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Image(systemName: systemImage)
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.body.weight(.medium))
                    .foregroundStyle(.primary)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: AppTheme.Spacing.sm)
            Text(trailing)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
        }
        .padding(AppTheme.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
        .contentShape(Rectangle())
    }

    // MARK: - Render orchestration

    private func renderImage() async {
        isRenderingImage = true
        lastError = nil
        defer { isRenderingImage = false }
        do {
            let url = try await ClipExporter.shared.exportImage(
                clip,
                episode: episode,
                podcast: podcast,
                theme: style
            )
            imageURL = url
        } catch {
            lastError = "Couldn't render image: \(error.localizedDescription)"
        }
    }

    private func renderAudio() async {
        isRenderingAudio = true
        lastError = nil
        defer { isRenderingAudio = false }
        do {
            let url = try await ClipExporter.shared.exportAudio(
                clip,
                episode: episode,
                podcast: podcast
            )
            audioURL = url
        } catch ClipExporter.ExportError.audioUnavailable {
            lastError = "Download this episode first — audio export needs the local file."
        } catch {
            lastError = "Couldn't render audio: \(error.localizedDescription)"
        }
    }

    // Video render orchestration intentionally omitted — see
    // `ClipVideoComposer` header for the punt details. The button is
    // disabled in `videoAction` so this code path isn't exercised.
}

// MARK: - Preview

#Preview {
    let podcastID = UUID()
    return ClipShareSheet(
        clip: Clip(
            episodeID: UUID(),
            subscriptionID: podcastID,
            startMs: 14 * 60_000 + 31_000,
            endMs: 14 * 60_000 + 58_000,
            transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria."
        ),
        episode: Episode(
            podcastID: podcastID,
            guid: "preview",
            title: "How to Think About Keto",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/x.mp3")!
        ),
        podcast: Podcast(
            id: podcastID,
            feedURL: URL(string: "https://example.com/feed")!,
            title: "The Tim Ferriss Show"
        )
    )
}
