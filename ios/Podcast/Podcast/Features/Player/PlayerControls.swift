import SwiftUI

// MARK: - PlayerControls
//
// Glass island at the bottom of the full-screen PlayerView. Owns the scrubber,
// transport row (back-15 / play-pause / forward-30) and action row (speed,
// sleep, AirPlay).
//
// Doctrine:
//   D7 — every interaction dispatches `podcast.player.*` and re-renders from
//        the next `PlayerState` snapshot. No optimistic local state, except
//        the scrubber drag (which holds a transient value during the gesture
//        and falls back to the snapshot on release).

struct PlayerControls: View {
    @Environment(KernelModel.self) private var model

    let player: PlayerState
    @Binding var scrubbingPosition: Double?
    @Binding var showSpeedSheet: Bool
    @Binding var showSleepSheet: Bool
    @Binding var showClipComposer: Bool

    private static let speeds: [Double] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0]

    var body: some View {
        VStack(spacing: PodcastSpace.l) {
            scrubber
            adSkipBar
            transport
            actionRow
        }
        .padding(.horizontal, PodcastSpace.l)
        .padding(.vertical, PodcastSpace.l)
        .background(.ultraThinMaterial)
        .clipShape(RoundedRectangle(cornerRadius: PodcastSpace.radius, style: .continuous))
        .shadow(color: .black.opacity(0.25), radius: 14, y: 6)
    }

    /// Render the skip-ad pill only while the playhead is inside one
    /// of the active episode's ad segments. Collapses to an empty
    /// view (zero footprint) otherwise so the transport row doesn't
    /// jump between ad and non-ad regions.
    @ViewBuilder
    private var adSkipBar: some View {
        if let segment = player.activeAdSegment(in: model.library) {
            HStack {
                AdSkipPill(segment: segment)
                Spacer()
            }
            .transition(.opacity)
        }
    }

    // MARK: - Scrubber

    private var displayPosition: Double {
        scrubbingPosition ?? player.positionSecs
    }

    private var duration: Double {
        player.durationSecs ?? 0
    }

    @ViewBuilder
    private var scrubber: some View {
        VStack(spacing: PodcastSpace.xs) {
            scrubberTrack
            HStack {
                Text(formatTime(displayPosition))
                Spacer()
                Text(remainingLabel)
            }
            .font(PodcastFont.caption.monospacedDigit())
            .foregroundStyle(PodcastColor.textSecondary)
        }
    }

    private var scrubberTrack: some View {
        GeometryReader { geo in
            let width = geo.size.width
            let fraction = duration > 0 ? min(max(displayPosition / duration, 0), 1) : 0
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(PodcastColor.hairline.opacity(0.5))
                    .frame(height: 4)
                Capsule()
                    .fill(PodcastColor.accent)
                    .frame(width: width * fraction, height: 4)
                Circle()
                    .fill(Color.white)
                    .frame(width: 14, height: 14)
                    .shadow(color: .black.opacity(0.2), radius: 2, y: 1)
                    .offset(x: width * fraction - 7)
            }
            .contentShape(Rectangle().inset(by: -PodcastSpace.s))
            .gesture(scrubGesture(in: width))
        }
        .frame(height: 18)
        .disabled(duration <= 0)
    }

    private func scrubGesture(in width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                guard duration > 0, width > 0 else { return }
                let pct = min(max(value.location.x / width, 0), 1)
                scrubbingPosition = pct * duration
            }
            .onEnded { _ in
                if let target = scrubbingPosition {
                    model.dispatch(namespace: "podcast.player", body: [
                        "op": "seek",
                        "position_secs": target,
                    ])
                }
                scrubbingPosition = nil
            }
    }

    private var remainingLabel: String {
        guard duration > 0 else { return "--:--" }
        let remaining = max(0, duration - displayPosition)
        return "-" + formatTime(remaining)
    }

    // MARK: - Transport

    private var transport: some View {
        HStack(spacing: PodcastSpace.xl) {
            transportButton(
                systemName: "gobackward.15",
                size: 30,
                accessibility: "Skip back 15 seconds"
            ) {
                model.dispatch(namespace: "podcast.player", body: ["op": "skip_backward", "secs": 15.0])
            }
            playPauseButton
            transportButton(
                systemName: "goforward.30",
                size: 30,
                accessibility: "Skip forward 30 seconds"
            ) {
                model.dispatch(namespace: "podcast.player", body: ["op": "skip_forward", "secs": 30.0])
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func transportButton(
        systemName: String,
        size: CGFloat,
        accessibility: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: size, weight: .semibold))
                .foregroundStyle(PodcastColor.textPrimary)
                .frame(width: 56, height: 56)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibility)
    }

    private var playPauseButton: some View {
        Button {
            if player.isPlaying {
                model.dispatch(namespace: "podcast.player", body: ["op": "pause"])
            } else if let epId = player.episodeId {
                model.dispatch(namespace: "podcast.player", body: [
                    "op": "play",
                    "episode_id": epId,
                ])
            }
        } label: {
            ZStack {
                if player.isBuffering {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .controlSize(.large)
                        .tint(PodcastColor.textPrimary)
                } else {
                    Image(systemName: player.isPlaying ? "pause.fill" : "play.fill")
                        .font(.system(size: 44, weight: .bold))
                        .foregroundStyle(PodcastColor.textPrimary)
                }
            }
            .frame(width: 72, height: 72)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(player.isPlaying ? "Pause" : "Play")
    }

    // MARK: - Action row

    private var actionRow: some View {
        HStack(spacing: PodcastSpace.m) {
            speedChip
            sleepChip
            clipChip
            Spacer(minLength: 0)
            AirPlayRoutePicker()
                .frame(width: 32, height: 32)
        }
    }

    private var clipChip: some View {
        Button {
            showClipComposer = true
        } label: {
            chipLabel(text: "Clip", systemImage: "scissors")
        }
        .buttonStyle(.plain)
        .accessibilityLabel("New clip")
    }

    private var speedChip: some View {
        Button {
            showSpeedSheet = true
        } label: {
            chipLabel(text: formatSpeed(player.speed), systemImage: "speedometer")
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Playback speed")
        .confirmationDialog("Playback speed", isPresented: $showSpeedSheet, titleVisibility: .visible) {
            ForEach(Self.speeds, id: \.self) { speed in
                Button(formatSpeed(speed)) {
                    model.dispatch(namespace: "podcast.player", body: [
                        "op": "set_speed",
                        "speed": Float(speed),
                    ])
                }
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private var sleepChip: some View {
        Button {
            showSleepSheet = true
        } label: {
            chipLabel(text: "Sleep", systemImage: "moon.zzz")
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Sleep timer")
        .confirmationDialog("Sleep timer", isPresented: $showSleepSheet, titleVisibility: .visible) {
            Button("5 minutes") { setSleepTimer(5 * 60) }
            Button("15 minutes") { setSleepTimer(15 * 60) }
            Button("30 minutes") { setSleepTimer(30 * 60) }
            Button("45 minutes") { setSleepTimer(45 * 60) }
            Button("1 hour") { setSleepTimer(60 * 60) }
            Button("Off", role: .destructive) {
                model.dispatch(namespace: "podcast.player", body: [
                    "op": "set_sleep_timer",
                    "secs": NSNull(),
                ])
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private func setSleepTimer(_ secs: Int) {
        model.dispatch(namespace: "podcast.player", body: [
            "op": "set_sleep_timer",
            "secs": secs,
        ])
    }

    private func chipLabel(text: String, systemImage: String) -> some View {
        HStack(spacing: PodcastSpace.xs) {
            Image(systemName: systemImage)
                .font(.system(size: 12, weight: .semibold))
            Text(text)
                .font(PodcastFont.caption.weight(.semibold))
        }
        .padding(.horizontal, PodcastSpace.m)
        .padding(.vertical, PodcastSpace.s)
        .foregroundStyle(PodcastColor.textPrimary)
        .background(PodcastColor.surface.opacity(0.85), in: Capsule())
    }

    // MARK: - Formatting

    private func formatTime(_ seconds: Double) -> String {
        guard seconds.isFinite, seconds >= 0 else { return "--:--" }
        let total = Int(seconds.rounded())
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 {
            return String(format: "%d:%02d:%02d", h, m, s)
        }
        return String(format: "%d:%02d", m, s)
    }

    private func formatSpeed(_ speed: Double) -> String {
        if abs(speed - speed.rounded()) < 0.01 {
            return String(format: "%.0f×", speed)
        }
        return String(format: "%.2g×", speed)
    }
}
