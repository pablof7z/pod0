import SwiftUI

extension AgentRun {
    var presentedFailure: ProductFailure? {
        guard let failureReason else { return nil }
        let fields = failureReason.split(separator: " ").reduce(into: [String: String]()) {
            result, field in
            let pieces = field.split(separator: "=", maxSplits: 1).map(String.init)
            guard pieces.count == 2 else { return }
            result[pieces[0]] = pieces[1]
        }
        let code = fields["failure_code"].flatMap(ProductFailureCode.init(rawValue:)) ?? .unexpected
        let diagnosticID = fields["diagnostic_id"]
            ?? String(id.uuidString.prefix(8)).uppercased()
        return ProductFailure(code: code, diagnosticID: diagnosticID)
    }
}

struct AgentRunFailureView: View {
    let failure: ProductFailure

    private var presented: UserFacingFailure {
        UserFacingFailurePresenter.make(failure: failure)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(presented.title, systemImage: "xmark.circle.fill")
                .font(.caption.weight(.semibold))
                .foregroundStyle(AppTheme.Tint.error)
            Text(presented.message)
                .font(.caption)
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let diagnosticID = presented.diagnosticID {
                Text("Diagnostic \(diagnosticID)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(AppTheme.Tint.error.opacity(0.12))
        )
    }
}
