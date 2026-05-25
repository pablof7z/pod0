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

    private var autoSkipAdsEnabled: Bool {
        model.podcastSnapshot?.settings?.autoSkipAdsEnabled ?? false
    }

    private var autoSkipBinding: Binding<Bool> {
        Binding(
            get: { autoSkipAdsEnabled },
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

    private var footerCopy: String {
        """
        When an episode has detected ad markers, the player will \
        seek past each ad break once per listening session. Scrubbing \
        back into a skipped ad will not auto-skip again.
        """
    }
}
