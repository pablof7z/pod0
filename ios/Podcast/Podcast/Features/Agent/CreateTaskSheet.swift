import SwiftUI

// MARK: - CreateTaskSheet
//
// Sheet presented from `AgentTasksView` toolbar `+`. Collects the four
// fields `podcast.tasks.create` needs and dispatches the action when
// Save is tapped. The action's reducer mints the task UUID; this view
// never invents an id locally.
//
// Action presets are kept simple: "Inbox Triage", "Categorize",
// and "Custom" (free-form namespace). Real receiver action modules
// don't exist yet — see `tasks_handler.rs::run_now` comment.

struct CreateTaskSheet: View {

    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var title: String = ""
    @State private var description: String = ""
    @State private var schedule: ScheduleOption = .daily
    @State private var actionPreset: ActionPreset = .inboxTriage
    @State private var customNamespace: String = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Title", text: $title)
                        .accessibilityLabel("Task title")
                    TextField("Description (optional)", text: $description, axis: .vertical)
                        .lineLimit(2...4)
                        .accessibilityLabel("Task description")
                }

                Section("Schedule") {
                    Picker("Schedule", selection: $schedule) {
                        ForEach(ScheduleOption.allCases, id: \.self) { option in
                            Text(option.label).tag(option)
                        }
                    }
                    .pickerStyle(.segmented)
                }

                Section("Action") {
                    Picker("Action", selection: $actionPreset) {
                        ForEach(ActionPreset.allCases, id: \.self) { preset in
                            Text(preset.label).tag(preset)
                        }
                    }
                    if actionPreset == .custom {
                        TextField("Namespace (e.g. podcast.research)",
                                  text: $customNamespace)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .accessibilityLabel("Custom action namespace")
                    }
                }
            }
            .navigationTitle("New Task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Save") { save() }
                        .fontWeight(.semibold)
                        .disabled(!canSave)
                }
            }
        }
    }

    // MARK: - Save

    private var canSave: Bool {
        let trimmedTitle = title.trimmingCharacters(in: .whitespaces)
        guard !trimmedTitle.isEmpty else { return false }
        if actionPreset == .custom {
            return !customNamespace.trimmingCharacters(in: .whitespaces).isEmpty
        }
        return true
    }

    private func save() {
        let trimmedTitle = title.trimmingCharacters(in: .whitespaces)
        let trimmedDescription = description.trimmingCharacters(in: .whitespaces)
        // Namespace + serialized action body must match the registered
        // receiver action modules in the Rust kernel. `action_body` is a
        // JSON *string* (the reducer re-parses it), so each preset carries
        // its own `{"op":…}` payload rather than an empty `{}`.
        let namespace: String
        let actionBody: String
        switch actionPreset {
        case .inboxTriage:
            namespace = "podcast.inbox"
            actionBody = #"{"op":"triage"}"#
        case .categorize:
            namespace = "podcast.categorize"
            actionBody = #"{"op":"run"}"#
        case .custom:
            namespace = customNamespace.trimmingCharacters(in: .whitespaces)
            actionBody = "{}"
        }
        var body: [String: Any] = [
            "op": "create",
            "title": trimmedTitle,
            "action_namespace": namespace,
            "action_body": actionBody,
            "schedule": schedule.rawValue,
        ]
        if !trimmedDescription.isEmpty {
            body["description"] = trimmedDescription
        }
        model.dispatch(namespace: "podcast.tasks", body: body)
        dismiss()
    }
}

// MARK: - Pickers

private enum ScheduleOption: String, CaseIterable {
    case daily, weekly, once
    var label: String {
        switch self {
        case .daily: "Daily"
        case .weekly: "Weekly"
        case .once: "Once"
        }
    }
}

private enum ActionPreset: CaseIterable {
    case inboxTriage, categorize, custom
    var label: String {
        switch self {
        case .inboxTriage: "Inbox Triage"
        case .categorize: "Categorize"
        case .custom: "Custom"
        }
    }
}
