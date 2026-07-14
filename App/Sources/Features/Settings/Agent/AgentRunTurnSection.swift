import SwiftUI

struct AgentRunTurnSection: View {
    let turn: AgentRunTurnData

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Turn \(turn.turnNumber)")
                    .font(.caption.weight(.semibold))
                    .tracking(1.2)
                    .textCase(.uppercase)
                    .foregroundStyle(.secondary)
                Spacer()
                if let response = turn.apiResponse {
                    Label(
                        "\(response.tokensUsed.promptTokens)→\(response.tokensUsed.completionTokens)",
                        systemImage: "function"
                    )
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                }
            }

            if let response = turn.apiResponse, let assistantText = Self.assistantText(in: response) {
                Text(assistantText)
                    .font(.caption)
                    .foregroundStyle(.primary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    .background(Color(.tertiarySystemBackground))
                    .cornerRadius(6)
            }

            if let response = turn.apiResponse, !response.toolCalls.isEmpty {
                Text("\(response.toolCalls.count) tool call\(response.toolCalls.count == 1 ? "" : "s") issued — see “Tools used” above for details")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }

            if turn.apiResponse == nil && turn.toolDispatches.isEmpty {
                Text("No assistant response recorded.")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    static func assistantText(in response: AgentAPIResponse) -> String? {
        guard let value = response.assistantMessage["content"] else { return nil }
        switch value {
        case .string(let s):
            let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
            return t.isEmpty ? nil : t
        case .array(let parts):
            let pieces: [String] = parts.compactMap { part in
                if case .object(let dict) = part,
                   case .string(let text) = dict["text"] ?? .null {
                    return text
                }
                return nil
            }
            let joined = pieces.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            return joined.isEmpty ? nil : joined
        default:
            return nil
        }
    }
}
