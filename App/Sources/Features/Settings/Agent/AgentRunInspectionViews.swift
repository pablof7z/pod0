import SwiftUI
import UIKit

struct AgentRunSystemPromptView: View {
    let systemPrompt: String

    @State private var copied = false

    var body: some View {
        ScrollView {
            Text(systemPrompt)
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        }
        .background(Color(.systemBackground))
        .navigationTitle("System Prompt")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    UIPasteboard.general.string = systemPrompt
                    withAnimation { copied = true }
                    Task {
                        try? await Task.sleep(nanoseconds: 1_200_000_000)
                        await MainActor.run { withAnimation { copied = false } }
                    }
                } label: {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc")
                }
            }
        }
    }
}

struct AgentRunMessagesView: View {
    let turns: [AgentRunTurnData]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                ForEach(turns) { turn in
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Turn \(turn.turnNumber)")
                            .font(.caption.weight(.semibold))
                            .tracking(1.2)
                            .textCase(.uppercase)
                            .foregroundStyle(.secondary)

                        Text(formatJSON(turn.messagesBeforeCall))
                            .font(.system(.caption2, design: .monospaced))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(12)
                            .background(Color(.tertiarySystemBackground))
                            .cornerRadius(8)
                    }
                }
                Color.clear.frame(height: 24)
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)
        }
        .background(Color(.systemBackground))
        .navigationTitle("Messages")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func formatJSON(_ array: [[String: AnyCodable]]) -> String {
        let jsonArray = array.map { $0.mapValues(anyCodableToAny) }
        guard
            JSONSerialization.isValidJSONObject(jsonArray),
            let data = try? JSONSerialization.data(
                withJSONObject: jsonArray,
                options: [.prettyPrinted, .sortedKeys]
            ),
            let str = String(data: data, encoding: .utf8)
        else { return String(describing: array) }
        return str
    }

    private func anyCodableToAny(_ value: AnyCodable) -> Any {
        switch value {
        case .null: return NSNull()
        case .bool(let b): return b
        case .int(let i): return i
        case .double(let d): return d
        case .string(let s): return s
        case .array(let arr): return arr.map(anyCodableToAny)
        case .object(let obj): return obj.mapValues(anyCodableToAny)
        }
    }
}

struct AgentRunShareItem: Identifiable {
    let id = UUID()
    let text: String
}

struct AgentRunShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
