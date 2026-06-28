import SwiftUI

struct NostrSearchHitRow: View {

    let hit: NostrSearchHit
    let isSubscribing: Bool
    let isAlreadySubscribed: Bool
    let rowError: String?
    let onSubscribe: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
                avatar
                VStack(alignment: .leading, spacing: 2) {
                    Text(hit.displayName)
                        .font(AppTheme.Typography.headline)
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                    Text(hit.detail)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                    Text(NostrNpub.shortNpub(fromHex: hit.author))
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                trailingControl
                    .padding(.top, 2)
            }
            if let rowError {
                Text(rowError)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.red)
                    .padding(.leading, 48 + AppTheme.Spacing.md)
            }
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture {
            guard !isSubscribing, !isAlreadySubscribed else { return }
            onSubscribe()
        }
        .opacity(isSubscribing || isAlreadySubscribed ? 0.65 : 1)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }

    private var avatar: some View {
        CachedAsyncImage(url: hit.profileMetadata?.picture.flatMap(URL.init(string:)),
                         targetSize: CGSize(width: 48, height: 48)) { phase in
            switch phase {
            case .success(let image):
                image.resizable().aspectRatio(contentMode: .fill)
            case .empty, .failure:
                ZStack {
                    Circle().fill(Color(.tertiarySystemFill))
                    Image(systemName: "person.crop.circle")
                        .foregroundStyle(.secondary)
                }
            @unknown default:
                Color(.tertiarySystemFill)
            }
        }
        .frame(width: 48, height: 48)
        .clipShape(Circle())
    }

    @ViewBuilder
    private var trailingControl: some View {
        if isSubscribing {
            ProgressView().controlSize(.small).frame(width: 32, height: 32)
        } else if isAlreadySubscribed {
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 32, height: 32)
        } else {
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        }
    }
}
