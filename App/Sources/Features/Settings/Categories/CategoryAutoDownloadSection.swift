import SwiftUI

// MARK: - CategoryAutoDownloadSection
//
// Reusable Form `Section` that renders the auto-download override knobs
// for a category. The "Override app default" toggle gates the existing
// 3-way picker (off / latestN / allNew) + Wi-Fi toggle + latest-N
// stepper UX from `ShowDetailSettingsSheet`, so we end up with the
// 4-way semantic (inherit / off / latestN / allNew) the spec calls for
// without writing a custom picker.
//
// Pure View; takes a `CategorySettings` snapshot + an update closure so
// the parent owns the store wiring.

struct CategoryAutoDownloadSection: View {
    let settings: CategorySettings
    let onUpdate: ((inout CategorySettings) -> Void) -> Void

    /// Picker-friendly enum mirroring the one in `ShowDetailSettingsSheet`.
    enum Choice: String, CaseIterable, Identifiable {
        case off
        case latestN
        case allNew

        var id: String { rawValue }

        var label: String {
            switch self {
            case .off:     return "Off"
            case .latestN: return "Latest"
            case .allNew:  return "All new"
            }
        }
    }

    @State private var choice: Choice
    @State private var latestNCount: Int
    @State private var wifiOnly: Bool
    @State private var overrideEnabled: Bool

    init(
        settings: CategorySettings,
        onUpdate: @escaping ((inout CategorySettings) -> Void) -> Void
    ) {
        self.settings = settings
        self.onUpdate = onUpdate
        let policy = settings.autoDownloadOverride ?? .default
        switch policy.mode {
        case .off:
            _choice = State(initialValue: .off)
            _latestNCount = State(initialValue: 5)
        case .latestN(let n):
            _choice = State(initialValue: .latestN)
            _latestNCount = State(initialValue: n)
        case .allNew:
            _choice = State(initialValue: .allNew)
            _latestNCount = State(initialValue: 5)
        }
        _wifiOnly = State(initialValue: policy.wifiOnly)
        _overrideEnabled = State(initialValue: settings.autoDownloadOverride != nil)
    }

    var body: some View {
        Section {
            Toggle("Override app default", isOn: $overrideEnabled)
                .onChange(of: overrideEnabled) { _, _ in persist() }

            if overrideEnabled {
                LiquidGlassSegmentedPicker(
                    "New episodes",
                    selection: $choice,
                    segments: Choice.allCases.map { ($0, $0.label) }
                )
                .listRowBackground(Color.clear)
                .listRowInsets(AppTheme.Layout.cardRowInsetsSM)
                .onChange(of: choice) { _, _ in persist() }

                if choice == .latestN {
                    Stepper(value: $latestNCount, in: 1...50) {
                        HStack {
                            Text("Keep latest")
                            Spacer()
                            Text("\(latestNCount)")
                                .foregroundStyle(.secondary)
                                .monospacedDigit()
                        }
                    }
                    .onChange(of: latestNCount) { _, _ in persist() }
                }

                if choice != .off {
                    Toggle("Wi-Fi only", isOn: $wifiOnly)
                        .onChange(of: wifiOnly) { _, _ in persist() }
                }
            }
        } header: {
            Text("Auto-download")
        } footer: {
            Text(footerText)
        }
    }

    private var footerText: String {
        if overrideEnabled {
            return "When on, every show in this category uses these settings instead of its individual auto-download policy."
        } else {
            return "Off — each show in this category continues to use its own auto-download policy."
        }
    }

    private func persist() {
        onUpdate { settings in
            guard overrideEnabled else {
                settings.autoDownloadOverride = nil
                return
            }
            let mode: AutoDownloadPolicy.Mode
            switch choice {
            case .off:     mode = .off
            case .latestN: mode = .latestN(latestNCount)
            case .allNew:  mode = .allNew
            }
            settings.autoDownloadOverride = AutoDownloadPolicy(mode: mode, wifiOnly: wifiOnly)
        }
    }
}
