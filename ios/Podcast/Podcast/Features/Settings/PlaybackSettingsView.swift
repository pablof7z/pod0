import SwiftUI

// MARK: - PlaybackSettingsView
//
// User-facing playback preferences mirrored from
// `PodcastUpdate.settings`. Every read goes through the kernel
// snapshot; every write dispatches `podcast.settings.*` and the next
// snapshot tick reflects the new value — no local mutable state.
//
// Doctrine:
//   D7 — Rust owns canonical settings. The Toggle binding sends the
//        new value upstream; the displayed state comes back via the
//        snapshot. A failed dispatch is silently ignored at the view
//        layer (the kernel emits a toast through the standard
//        snapshot.toast channel).

struct PlaybackSettingsView: View {
    @Environment(KernelModel.self) private var model

    private static let skipOptions: [Int] = [5, 10, 15, 20, 30, 45, 60, 90, 120]

    private var settings: SettingsSnapshot { model.settings }

    private var autoSkipBinding: Binding<Bool> {
        Binding(
            get: { settings.autoSkipAdsEnabled },
            set: { newValue in
                model.dispatch(namespace: "podcast.settings", body: [
                    "op": "set_auto_skip_ads",
                    "enabled": newValue,
                ])
            }
        )
    }

    var body: some View {
        Form {
            Section {
                Picker("Skip Forward", selection: skipForwardBinding) {
                    ForEach(Self.skipOptions, id: \.self) { sec in
                        Text("\(sec)s").tag(sec)
                    }
                }
                Picker("Skip Backward", selection: skipBackwardBinding) {
                    ForEach(Self.skipOptions, id: \.self) { sec in
                        Text("\(sec)s").tag(sec)
                    }
                }
            } header: {
                Text("Skip Intervals")
            } footer: {
                Text("Sets the duration for the skip-forward and skip-backward buttons in the player.")
                    .font(PodcastFont.caption)
                    .foregroundStyle(.secondary)
            }

            Section {
                Toggle("Skip Ads Automatically", isOn: autoSkipBinding)
                    .accessibilityIdentifier("auto-skip-ads-toggle")
            } footer: {
                Text(footerCopy)
                    .font(PodcastFont.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Playback")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var skipForwardBinding: Binding<Int> {
        Binding(
            get: { Int(settings.skipForwardSecs) },
            set: { newSecs in
                model.dispatch(namespace: "podcast.settings", body: [
                    "op": "set_skip_intervals",
                    "forward_secs": Double(newSecs),
                    "backward_secs": settings.skipBackwardSecs,
                ])
            }
        )
    }

    private var skipBackwardBinding: Binding<Int> {
        Binding(
            get: { Int(settings.skipBackwardSecs) },
            set: { newSecs in
                model.dispatch(namespace: "podcast.settings", body: [
                    "op": "set_skip_intervals",
                    "forward_secs": settings.skipForwardSecs,
                    "backward_secs": Double(newSecs),
                ])
            }
        )
    }

    private var footerCopy: String {
        """
        When an episode has detected ad markers, the player will \
        seek past each ad break once per listening session. Scrubbing \
        back into a skipped ad will not auto-skip again.
        """
    }
}
