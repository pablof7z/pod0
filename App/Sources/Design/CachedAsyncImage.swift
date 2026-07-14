import Kingfisher
import SwiftUI

/// Drop-in caching replacement for SwiftUI's `AsyncImage`, backed by
/// Kingfisher's memory + disk cache.
///
/// Same call signature as `AsyncImage` so swapping is a one-token edit:
///
/// ```swift
/// CachedAsyncImage(url: artworkURL) { phase in
///     switch phase {
///     case .success(let image): image.resizable().scaledToFill()
///     case .failure:            Color.secondary.opacity(0.1)
///     case .empty:              ProgressView()
///     @unknown default:         EmptyView()
///     }
/// }
/// ```
///
/// SwiftUI's stock `AsyncImage` re-downloads on every appearance and does
/// not share its cache across views. Kingfisher uses a managed cache so
/// the same artwork URL fetches at most once per session — which matters
/// when the user scrolls a Discover list, hops to Library, opens a show,
/// and lands on Now Playing all in 30 seconds.
struct CachedAsyncImage<Content: View>: View {

    let url: URL?
    let scale: CGFloat
    /// Optional logical-points target size. When set, the image is decoded
    /// at the smallest pixel size that covers `targetSize × screen-scale`
    /// instead of being kept at its source resolution. Cuts memory ~9× for
    /// thumbnails (e.g. a 600×600 iTunes artwork tile rendered at 64pt
    /// drops from ~1.4 MB to ~150 KB in memory).
    let targetSize: CGSize?
    @ViewBuilder let content: (AsyncImagePhase) -> Content

    @State private var phase: AsyncImagePhase = .empty
    @State private var task: DownloadTask?

    init(
        url: URL?,
        scale: CGFloat = 1,
        targetSize: CGSize? = nil,
        @ViewBuilder content: @escaping (AsyncImagePhase) -> Content
    ) {
        self.url = url
        self.scale = scale
        self.targetSize = targetSize
        self.content = content
    }

    var body: some View {
        content(phase)
            .onChange(of: url, initial: true) { _, newURL in
                load(newURL)
            }
            .onDisappear {
                task?.cancel()
                task = nil
            }
    }

    private func load(_ url: URL?) {
        task?.cancel()
        guard let url else {
            phase = .empty
            return
        }

        // If the image is already in the memory cache, complete synchronously
        // so the view never flickers through `.empty`. Disk-cache hits still
        // go through the async path but typically resolve in one frame.
        //
        // The cache key includes any downsampling target size so two callers
        // requesting different sizes for the same URL each get their own
        // memory entry instead of fighting over one decoded variant.
        let cache = ImageCache.default
        let baseKey = url.absoluteString
        let cacheKey = targetSize.map { "\(baseKey)|ds=\(Int($0.width))x\(Int($0.height))" } ?? baseKey
        if cache.isCached(forKey: cacheKey),
           let cached = cache.retrieveImageInMemoryCache(forKey: cacheKey) {
            phase = .success(Image(uiImage: cached))
            return
        }

        phase = .empty
        var options: KingfisherOptionsInfo = [
            .transition(.fade(0.15)),
            .scaleFactor(scale),
        ]
        if let targetSize {
            // Kingfisher's processor expects pixels; convert from points.
            let screenScale = UIScreen.main.scale
            let pixelSize = CGSize(
                width: targetSize.width * screenScale,
                height: targetSize.height * screenScale
            )
            options.append(.processor(DownsamplingImageProcessor(size: pixelSize)))
            options.append(.cacheOriginalImage)
            options.append(.cacheSerializer(FormatIndicatedCacheSerializer.png))
        }

        task = KingfisherManager.shared.retrieveImage(
            with: url,
            options: options
        ) { result in
            switch result {
            case .success(let value):
                phase = .success(Image(uiImage: value.image))
            case .failure(let error):
                // `.cancelled` happens any time the URL changes mid-load
                // (scrolling); collapse it to `.empty` rather than surfacing
                // it as a failure to the caller's switch.
                if error.isTaskCancelled || error.isNotCurrentTask {
                    return
                }
                phase = .failure(error)
            }
        }
    }
}
