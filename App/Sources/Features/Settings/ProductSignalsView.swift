import SwiftUI

struct ProductSignalsView: View {
    @State private var snapshot: ProductSignalSnapshot?
    @State private var isLoading = true
    @State private var showDeleteConfirmation = false
    @State private var errorMessage: String?

    var body: some View {
        List {
            privacySection
            controlsSection
            if let snapshot {
                summarySection(snapshot)
                countsSection(snapshot)
                recentSection(snapshot)
            } else if isLoading {
                ProgressView("Loading signals…")
            }
        }
        .settingsListStyle()
        .navigationTitle("Product Signals")
        .navigationBarTitleDisplayMode(.inline)
        .task { await refresh() }
        .alert("Delete Product Signals?", isPresented: $showDeleteConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) { deleteSignals() }
        } message: {
            Text("This permanently removes every local product signal and creates a new anonymous install identifier.")
        }
    }

    private var privacySection: some View {
        Section {
            Label("Stored only on this device", systemImage: "lock.shield")
            Text("Signals contain event names, outcomes, coarse timing, and error classes. They never contain titles, transcript text, searches, notes, or clips.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        } footer: {
            Text("Pod0 sends nothing automatically. Exporting always opens the system share sheet for your approval.")
        }
    }

    private var controlsSection: some View {
        Section("Controls") {
            Toggle("Collect product signals", isOn: enabledBinding)
                .disabled(snapshot == nil)
            Button {
                exportSignals()
            } label: {
                Label("Export Signals", systemImage: "square.and.arrow.up")
            }
            .disabled(snapshot?.signals.isEmpty != false)
            Button("Delete Signals", role: .destructive) {
                showDeleteConfirmation = true
            }
            .disabled(snapshot?.signals.isEmpty != false)
            if let errorMessage {
                Text(errorMessage)
                    .font(.footnote)
                    .foregroundStyle(AppTheme.Tint.error)
            }
        }
    }

    private func summarySection(_ snapshot: ProductSignalSnapshot) -> some View {
        let report = snapshot.report
        return Section("Summary") {
            SettingsRow(icon: "waveform.path.ecg", tint: .indigo, title: "Signals", value: "\(report.signalCount)")
            SettingsRow(icon: "calendar", tint: .blue, title: "Active days", value: "\(report.distinctActiveDays)")
            SettingsRow(
                icon: "person.crop.circle.badge.questionmark",
                tint: .gray,
                title: "Anonymous install",
                value: String(snapshot.anonymousInstallID.uuidString.prefix(8))
            )
        }
    }

    private func countsSection(_ snapshot: ProductSignalSnapshot) -> some View {
        Section("Outcomes") {
            if snapshot.report.counts.isEmpty {
                Text("No signals recorded yet")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(snapshot.report.counts) { item in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.name.displayName)
                            Text(item.outcome.displayName)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text("\(item.count)")
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func recentSection(_ snapshot: ProductSignalSnapshot) -> some View {
        Section("Recent") {
            ForEach(snapshot.signals.prefix(20)) { signal in
                VStack(alignment: .leading, spacing: 3) {
                    Text(signal.name.displayName)
                    HStack {
                        Text(signal.outcome.displayName)
                        Spacer()
                        Text(signal.occurredAt, format: .dateTime.month().day().hour().minute())
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var enabledBinding: Binding<Bool> {
        Binding(
            get: { snapshot?.isEnabled ?? false },
            set: { enabled in
                Task {
                    await ProductSignalStore.shared.setEnabled(enabled)
                    await refresh()
                }
            }
        )
    }

    private func refresh() async {
        snapshot = await ProductSignalStore.shared.snapshot()
        isLoading = false
    }

    private func deleteSignals() {
        Task {
            await ProductSignalStore.shared.deleteAll()
            await refresh()
            Haptics.success()
        }
    }

    private func exportSignals() {
        Task {
            guard let data = await ProductSignalStore.shared.exportData() else {
                errorMessage = "Pod0 couldn't prepare the export safely."
                Haptics.error()
                return
            }
            do {
                let url = FileManager.default.temporaryDirectory
                    .appendingPathComponent("pod0-product-signals.json")
                try data.write(to: url, options: .atomic)
                errorMessage = nil
                Haptics.success()
                await MainActor.run { SystemShareSheet.present(items: [url]) }
            } catch {
                errorMessage = "Pod0 couldn't prepare the export safely."
                Haptics.error()
            }
        }
    }
}

private extension ProductSignalName {
    var displayName: String {
        switch self {
        case .appLaunch: "App launch"
        case .firstSubscription: "First subscription"
        case .playStarted: "Play started"
        case .meaningfulListening: "Meaningful listening"
        case .resumeAttempt: "Resume attempt"
        case .playbackError: "Playback error"
        case .transcriptReady: "Transcript ready"
        case .transcriptUsed: "Transcript used"
        case .recallAsked: "Recall asked"
        case .recallGrounded: "Recall grounded"
        case .recallCitationOpened: "Citation opened"
        case .recallShadowParity: "Recall shadow parity"
        case .noteCreated: "Note created"
        case .clipCreated: "Clip created"
        case .agentTurnCompleted: "Agent turn completed"
        case .uncleanTermination: "Unclean termination"
        case .dataLossEvidence: "Data-loss evidence"
        }
    }
}

private extension ProductSignalOutcome {
    var displayName: String {
        rawValue.replacingOccurrences(of: "([a-z])([A-Z])", with: "$1 $2", options: .regularExpression).capitalized
    }
}
