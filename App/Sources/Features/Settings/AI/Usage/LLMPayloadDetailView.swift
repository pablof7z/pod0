import SwiftUI
import UIKit

struct LLMPayloadDetailView: View {
    let record: UsageRecord

    @State private var copyFlash: String?
    @State private var shareItem: ShareItem?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                headerSection
                if let payloadJSON = record.requestPayloadJSON {
                    payloadSection(title: "Request", content: payloadJSON)
                }
                if let response = record.responseContentPreview {
                    payloadSection(title: "Response", content: response)
                }
                metadataSection
                Color.clear.frame(height: 24)
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
        }
        .background(Color(.systemBackground))
        .navigationTitle("Call Details")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button { copyAll() } label: {
                        Label("Copy full JSON", systemImage: "doc.on.doc")
                    }
                    if let payload = record.requestPayloadJSON {
                        Button { copy(formatJSON(payload), label: "Copied request") } label: {
                            Label("Copy request only", systemImage: "arrow.up.doc")
                        }
                    }
                    if let response = record.responseContentPreview {
                        Button { copy(formatJSON(response), label: "Copied response") } label: {
                            Label("Copy response only", systemImage: "arrow.down.doc")
                        }
                    }
                    Divider()
                    Button { shareItem = ShareItem(text: exportText()) } label: {
                        Label("Share…", systemImage: "square.and.arrow.up")
                    }
                } label: {
                    Image(systemName: "square.and.arrow.up")
                }
            }
        }
        .overlay(alignment: .top) {
            if let copyFlash {
                Text(copyFlash)
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(.ultraThinMaterial, in: Capsule())
                    .padding(.top, 8)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .sheet(item: $shareItem) { item in
            ShareSheet(items: [item.text])
        }
    }

    // MARK: - Sections

    private var headerSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(CostFeature.displayName(for: record.feature))
                    .font(.headline).foregroundStyle(.primary)
                Spacer()
                Text(CostFormatter.usd(record.costUSD))
                    .font(.headline.monospacedDigit()).foregroundStyle(.primary)
            }
            VStack(alignment: .leading, spacing: 8) {
                detailRow("Model", record.model)
                detailRow("Time", record.at.formatted(date: .abbreviated, time: .standard))
                detailRow("Latency", CostFormatter.latency(record.latencyMs))
                detailRow("Tokens", "\(record.promptTokens) → \(record.completionTokens)")
            }
            .font(.caption).foregroundStyle(.secondary)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    private func payloadSection(title: String, content: String) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            sectionLabel(title)
            Text(formatJSON(content))
                .font(.system(.caption2, design: .monospaced))
                .foregroundStyle(.primary)
                .textSelection(.enabled)
                .lineLimit(.max)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
                .background(Color(.tertiarySystemBackground))
                .cornerRadius(8)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    private var metadataSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            sectionLabel("Metadata")
            VStack(alignment: .leading, spacing: 8) {
                detailRow("ID", record.id.uuidString.lowercased())
                if record.cachedTokens > 0 {
                    detailRow("Cached tokens", record.cachedTokens.formatted())
                }
                if record.reasoningTokens > 0 {
                    detailRow("Reasoning tokens", record.reasoningTokens.formatted())
                }
            }
            .font(.caption).foregroundStyle(.secondary)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
    }

    // MARK: - Helpers

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.caption.weight(.semibold))
            .tracking(1.2)
            .textCase(.uppercase)
            .foregroundStyle(.secondary)
    }

    private func detailRow(_ label: String, _ value: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(label).lineLimit(1).foregroundStyle(.secondary)
            Spacer()
            Text(value).lineLimit(2).truncationMode(.middle).textSelection(.enabled)
        }
    }

    private func formatJSON(_ str: String) -> String {
        guard let data = str.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data),
              let pretty = try? JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted, .sortedKeys]),
              let formatted = String(data: pretty, encoding: .utf8)
        else { return str }
        return formatted
    }

    // MARK: - Export

    private func copyAll() {
        copy(exportText(), label: "Copied full JSON")
    }

    private func copy(_ text: String, label: String) {
        UIPasteboard.general.string = text
        showFlash(label)
    }

    private func showFlash(_ message: String) {
        withAnimation(.easeOut(duration: 0.18)) { copyFlash = message }
        Task {
            try? await Task.sleep(nanoseconds: 1_400_000_000)
            await MainActor.run {
                withAnimation(.easeIn(duration: 0.25)) { copyFlash = nil }
            }
        }
    }

    private func exportText() -> String {
        var dict: [String: Any] = [
            "id": record.id.uuidString,
            "at": ISO8601DateFormatter().string(from: record.at),
            "feature": record.feature,
            "featureDisplay": CostFeature.displayName(for: record.feature),
            "model": record.model,
            "promptTokens": record.promptTokens,
            "completionTokens": record.completionTokens,
            "cachedTokens": record.cachedTokens,
            "reasoningTokens": record.reasoningTokens,
            "costUSD": record.costUSD,
            "latencyMs": record.latencyMs,
        ]
        if let req = record.requestPayloadJSON { dict["request"] = decodeJSONOrString(req) }
        if let resp = record.responseContentPreview { dict["response"] = decodeJSONOrString(resp) }
        guard JSONSerialization.isValidJSONObject(dict),
              let data = try? JSONSerialization.data(withJSONObject: dict, options: [.prettyPrinted, .sortedKeys]),
              let str = String(data: data, encoding: .utf8) else { return String(describing: dict) }
        return str
    }

    private func decodeJSONOrString(_ s: String) -> Any {
        guard let data = s.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) else { return s }
        return obj
    }
}

// MARK: - Share helpers

private struct ShareItem: Identifiable {
    let id = UUID()
    let text: String
}

// `ShareSheet` lives in `App/Sources/Design/ShareSheet.swift` — same
// signature, reused here so the type-name doesn't collide.
