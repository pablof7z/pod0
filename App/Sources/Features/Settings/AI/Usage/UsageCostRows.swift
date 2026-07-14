import SwiftUI

// MARK: - Bucket row

struct UsageBucketRow: View {
    let bucket: CostBucket
    let maxCost: Double

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(bucket.name)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                Text(CostFormatter.usd(bucket.cost))
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.primary)
            }

            GeometryReader { geo in
                let fraction = max(0.02, CGFloat(bucket.cost / maxCost))
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(Color(.tertiarySystemFill))
                        .frame(height: 6)
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(Color.accentColor)
                        .frame(width: geo.size.width * fraction, height: 6)
                }
            }
            .frame(height: 6)

            HStack(spacing: 8) {
                Text("\(bucket.count) calls")
                Text("·")
                Text("avg \(CostFormatter.usdCompact(bucket.cost / Double(max(bucket.count, 1))))")
                if bucket.cachedTokens > 0 {
                    Text("·")
                    Text("\(bucket.cachedTokens.formatted()) cached")
                }
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Recent call row

struct UsageRecentRow: View {
    let record: UsageRecord

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(CostFeature.displayName(for: record.feature))
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)

                Text(record.model)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)

                HStack(spacing: 6) {
                    Text(record.at.formatted(date: .abbreviated, time: .shortened))
                    Text("·")
                    if let seconds = record.audioDurationSeconds, seconds > 0 {
                        // STT-shaped record — show audio duration instead of
                        // tokens (which are usually zero on STT and would just
                        // read "0→0 tok").
                        Text(formatAudioDuration(seconds))
                    } else {
                        Text("\(record.promptTokens.formatted())→\(record.completionTokens.formatted()) tok")
                    }
                    if record.cachedTokens > 0 {
                        Text("·")
                        Text("\(record.cachedTokens.formatted()) cached")
                    }
                    if record.reasoningTokens > 0 {
                        Text("·")
                        Text("\(record.reasoningTokens.formatted()) reasoning")
                    }
                    Text("·")
                    Text(CostFormatter.latency(record.latencyMs))
                }
                .font(.caption2)
                .foregroundStyle(.tertiary)
            }

            Spacer()

            HStack(spacing: 6) {
                Text(CostFormatter.usd(record.costUSD))
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.primary)
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 10)
    }

    /// `9.2s` / `1m 23s` / `1h 4m`.
    private func formatAudioDuration(_ seconds: Double) -> String {
        let total = Int(seconds.rounded())
        if total < 60 {
            return seconds < 10
                ? String(format: "%.1fs audio", seconds)
                : "\(total)s audio"
        }
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 { return "\(h)h \(m)m audio" }
        return "\(m)m \(s)s audio"
    }
}
