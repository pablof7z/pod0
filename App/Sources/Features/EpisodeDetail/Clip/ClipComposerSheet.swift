import SwiftUI

// MARK: - ClipComposerSheet

/// New-clip composer per UX-03 §6.6. Opens pre-populated with the sentence
/// the user long-pressed. Drag handles widen / narrow the range against the
/// surrounding sentence list (sentence-snap by default; word-snap is a
/// future toggle). Caption + speaker-label + style toggles drive the live
/// preview rendered by `ClipPreviewView`.
///
/// Save persists through the Rust `podcast.clip.create` action and dismisses.
/// Share dispatches the same kernel create action, then presents the native
/// share/export capability surface for the committed clip.
struct ClipComposerSheet: View {

    // MARK: Inputs

    let episode: Episode
    let transcript: Transcript
    let initialSegment: Segment

    // MARK: Environment

    @Environment(\.dismiss) private var dismiss
    @Environment(AppStateStore.self) private var store

    // MARK: Draft state

    @State private var startMs: Int = 0
    @State private var endMs: Int = 0
    @State private var caption: String = ""
    @State private var showSpeakerLabel: Bool = true
    @State private var subtitleStyle: ClipSubtitleStyle = .editorial
    @State private var pendingShareClip: PendingShareClip?

    private struct PendingShareClip: Identifiable {
        let id: UUID
    }

    // MARK: Body

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                    ClipPreviewView(
                        transcriptText: currentTranscriptText,
                        speakerLabel: speakerDisplayName,
                        timestampLabel: timestampLabel,
                        caption: caption.isEmpty ? nil : caption,
                        style: subtitleStyle,
                        showSpeakerLabel: showSpeakerLabel
                    )

                    handlesSection
                    captionField
                    togglesSection
                }
                .padding(AppTheme.Spacing.lg)
            }
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .safeAreaInset(edge: .bottom) { actionBar }
            .navigationTitle("New Clip")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") { dismiss() }
                }
            }
            .onAppear(perform: bootstrapDraft)
            .sheet(item: $pendingShareClip) { pending in
                if let clip = store.clip(id: pending.id),
                   let podcast = store.podcast(id: episode.podcastID) {
                    ClipShareSheet(clip: clip, episode: episode, podcast: podcast)
                } else if store.clip(id: pending.id) == nil {
                    sharePreparingPlaceholder
                } else {
                    shareUnavailablePlaceholder
                }
            }
        }
    }

    // MARK: - Sections

    private var handlesSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            HStack {
                Text("Range")
                    .font(.system(.subheadline, design: .rounded).weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(timestampLabel)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            ClipComposerHandlesView(
                segments: surroundingSegments,
                startMs: $startMs,
                endMs: $endMs
            )
        }
        .padding(.horizontal, AppTheme.Spacing.sm)
    }

    private var captionField: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text("Caption")
                .font(.system(.subheadline, design: .rounded).weight(.semibold))
                .foregroundStyle(.secondary)
            TextField("Optional headline", text: $caption, axis: .vertical)
                .lineLimit(1...3)
                .padding(AppTheme.Spacing.md)
                .background(
                    RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                        .fill(Color(.systemBackground))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                        .strokeBorder(Color.secondary.opacity(0.18), lineWidth: 0.5)
                )
        }
        .padding(.horizontal, AppTheme.Spacing.sm)
    }

    private var togglesSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            LiquidGlassSegmentedPicker(
                "Subtitle style",
                selection: $subtitleStyle,
                segments: ClipSubtitleStyle.allCases.map { ($0, $0.label) }
            )

            Toggle(isOn: $showSpeakerLabel) {
                Text("Show speaker label")
                    .font(.system(.subheadline, design: .rounded))
            }
            .disabled(speakerDisplayName == nil)
        }
        .padding(.horizontal, AppTheme.Spacing.sm)
    }

    private var actionBar: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Button(action: save) {
                Text("Save")
                    .font(.system(.body, design: .rounded).weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                            .fill(Color(.secondarySystemBackground))
                    )
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)

            Button(action: share) {
                Text("Share")
                    .font(.system(.body, design: .rounded).weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                            .fill(Color.accentColor)
                    )
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color(.systemBackground))
        }
        .padding(AppTheme.Spacing.md)
        .background(.ultraThinMaterial)
    }

    // MARK: - Share fallback

    private var shareUnavailablePlaceholder: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            Image(systemName: "square.and.arrow.up")
                .font(.system(size: 44))
                .foregroundStyle(.secondary)
            Text("Share unavailable")
                .font(.system(.headline, design: .rounded))
            Text("The source show is no longer available in your library.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button("Done") { pendingShareClip = nil; dismiss() }
                .buttonStyle(.borderedProminent)
        }
        .padding(AppTheme.Spacing.xl)
        .presentationDetents([.medium])
    }

    private var sharePreparingPlaceholder: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            ProgressView()
                .controlSize(.large)
            Text("Preparing clip")
                .font(.system(.headline, design: .rounded))
            Text("The kernel is saving the clip before sharing.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button("Done") { pendingShareClip = nil; dismiss() }
                .buttonStyle(.borderedProminent)
        }
        .padding(AppTheme.Spacing.xl)
        .presentationDetents([.medium])
    }

    // MARK: - Lifecycle

    private func bootstrapDraft() {
        startMs = Int(initialSegment.start * 1000)
        endMs   = Int(initialSegment.end * 1000)
        caption = ""
        showSpeakerLabel = speakerDisplayName != nil
        subtitleStyle = .editorial
    }

    // MARK: - Actions

    private func save() {
        let clipID = UUID()
        store.kernelCreateClip(
            id: clipID,
            episodeID: episode.id,
            startSecs: TimeInterval(startMs) / 1000,
            endSecs: TimeInterval(endMs) / 1000,
            title: caption.isEmpty ? nil : caption,
            source: .touch
        )
        Haptics.success()
        dismiss()
    }

    private func share() {
        let clipID = UUID()
        store.kernelCreateClip(
            id: clipID,
            episodeID: episode.id,
            startSecs: TimeInterval(startMs) / 1000,
            endSecs: TimeInterval(endMs) / 1000,
            title: caption.isEmpty ? nil : caption,
            source: .touch
        )
        pendingShareClip = PendingShareClip(id: clipID)
    }

    // MARK: - Derived

    /// Segments overlapping the current `[startMs, endMs]` range. Drives the
    /// live transcript-text preview as the user widens / narrows.
    private var selectedSegments: [Segment] {
        let lo = TimeInterval(startMs) / 1000
        let hi = TimeInterval(endMs) / 1000
        return transcript.segments.filter { seg in
            seg.end >= lo && seg.start <= hi
        }
    }

    /// Window of segments shown in the handles track. We surround the
    /// initial sentence with a few neighbours so the user can drag outward
    /// without the track running off the screen on either edge.
    private var surroundingSegments: [Segment] {
        let allSegs = transcript.segments
        guard let centerIdx = allSegs.firstIndex(where: { $0.id == initialSegment.id }) else {
            return allSegs
        }
        let lower = max(0, centerIdx - 4)
        let upper = min(allSegs.count, centerIdx + 5)
        return Array(allSegs[lower..<upper])
    }

    private var currentTranscriptText: String {
        let texts = selectedSegments.map(\.text)
        return texts.isEmpty ? initialSegment.text : texts.joined(separator: " ")
    }

    private var speakerDisplayName: String? {
        // Only label the clip when every selected segment shares the same
        // speaker — mixed-speaker spans are intentionally unlabelled, since
        // a single name on a back-and-forth would mislead the reader.
        let ids = Set(selectedSegments.compactMap(\.speakerID))
        guard ids.count == 1, let only = ids.first else { return nil }
        let speaker = transcript.speaker(for: only)
        return speaker?.displayName ?? speaker?.label
    }

    private var timestampLabel: String {
        "\(format(ms: startMs)) \u{2192} \(format(ms: endMs))"
    }

    private func format(ms: Int) -> String {
        let total = ms / 1000
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }
}

// MARK: - Preview

#Preview {
    let store = AppStateStore()
    let subID = UUID()
    let episode = Episode(
        podcastID: subID,
        guid: "preview-clip",
        title: "How to Think About Keto",
        pubDate: Date(timeIntervalSince1970: 1_714_780_800),
        duration: 60 * 60,
        enclosureURL: URL(string: "https://example.com/file.mp3")!
    )
    let peter = Speaker(label: "Peter Attia", displayName: "Peter Attia")
    let segs = (0..<8).map { i in
        Segment(
            start: TimeInterval(i) * 6,
            end: TimeInterval(i) * 6 + 5,
            speakerID: peter.id,
            text: "Segment \(i): metabolic flexibility is a property of the mitochondria."
        )
    }
    let transcript = Transcript(
        episodeID: episode.id,
        language: "en-US",
        source: .scribeV1,
        segments: segs,
        speakers: [peter]
    )
    return ClipComposerSheet(
        episode: episode,
        transcript: transcript,
        initialSegment: segs[3]
    )
    .environment(store)
}
