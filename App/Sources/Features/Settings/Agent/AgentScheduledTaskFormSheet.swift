import SwiftUI

// MARK: - AgentScheduledTaskFormSheet

/// Modal form for creating or editing a scheduled task.
/// Presents label, prompt, and interval fields.
struct AgentScheduledTaskFormSheet: View {

    enum Mode {
        case create
        case edit(AgentScheduledTask)
    }

    // MARK: - Constants

    private enum Preset {
        static let options: [(label: String, seconds: TimeInterval)] = [
            ("Hourly",  3_600),
            ("Daily",   86_400),
            ("Weekly",  604_800),
        ]
    }

    // MARK: - Properties

    let mode: Mode
    let onSave: (String, String, TimeInterval) -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var label: String = ""
    @State private var prompt: String = ""
    @State private var selectedPreset: TimeInterval? = 86_400
    @State private var customSeconds: String = ""
    @State private var useCustom: Bool = false

    @FocusState private var promptFocused: Bool

    // MARK: - Derived

    private var titleText: String {
        switch mode {
        case .create: return "New Task"
        case .edit:   return "Edit Task"
        }
    }

    private var resolvedInterval: TimeInterval? {
        if useCustom {
            return Double(customSeconds).map { $0 > 0 ? $0 : nil } ?? nil
        }
        return selectedPreset
    }

    private var saveDisabled: Bool {
        label.trimmed.isEmpty || prompt.trimmed.isEmpty || resolvedInterval == nil
    }

    // MARK: - Body

    var body: some View {
        NavigationStack {
            Form {
                labelSection
                promptSection
                intervalSection
            }
            .navigationTitle(titleText)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { commitSave() }
                        .disabled(saveDisabled)
                        .fontWeight(.semibold)
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onAppear { seedFromMode() }
    }

    // MARK: - Sections

    private var labelSection: some View {
        Section("Label") {
            TextField("Short name for this task", text: $label)
                .font(AppTheme.Typography.body)
        }
    }

    private var promptSection: some View {
        Section {
            TextEditor(text: $prompt)
                .focused($promptFocused)
                .font(AppTheme.Typography.body)
                .frame(minHeight: 80)
        } header: {
            Text("Prompt")
        } footer: {
            Text("The agent will run this prompt on the schedule you choose.")
        }
    }

    private var intervalSection: some View {
        Section("Interval") {
            ForEach(Preset.options, id: \.seconds) { option in
                Button {
                    selectedPreset = option.seconds
                    useCustom = false
                } label: {
                    HStack {
                        Text(option.label)
                            .foregroundStyle(.primary)
                        Spacer()
                        if !useCustom && selectedPreset == option.seconds {
                            Image(systemName: "checkmark")
                                .foregroundStyle(.tint)
                                .font(AppTheme.Typography.caption.weight(.semibold))
                        }
                    }
                }
            }

            Button {
                useCustom = true
            } label: {
                HStack {
                    Text("Custom")
                        .foregroundStyle(.primary)
                    Spacer()
                    if useCustom {
                        Image(systemName: "checkmark")
                            .foregroundStyle(.tint)
                            .font(AppTheme.Typography.caption.weight(.semibold))
                    }
                }
            }

            if useCustom {
                HStack {
                    TextField("Seconds", text: $customSeconds)
                        .keyboardType(.numberPad)
                        .font(AppTheme.Typography.body)
                    Text("seconds")
                        .foregroundStyle(.secondary)
                        .font(AppTheme.Typography.callout)
                }
            }
        }
    }

    // MARK: - Helpers

    private func seedFromMode() {
        if case .edit(let task) = mode {
            label = task.label
            prompt = task.prompt
            let knownPreset = Preset.options.first { $0.seconds == task.intervalSeconds }
            if let preset = knownPreset {
                selectedPreset = preset.seconds
                useCustom = false
            } else {
                useCustom = true
                customSeconds = "\(Int(task.intervalSeconds))"
            }
        }
    }

    private func commitSave() {
        guard let interval = resolvedInterval else { return }
        onSave(label.trimmed, prompt.trimmed, interval)
        Haptics.success()
        dismiss()
    }
}
