import Foundation
import Kingfisher

/// Bounds Kingfisher's image cache so artwork doesn't grow unchecked.
///
/// Default Kingfisher caches use `NSCache` for memory (which evicts under
/// pressure but has no explicit byte cap) and 1 GB disk with 1-week TTL.
/// For a podcast app where the same handful of show covers churn through
/// every list, we want tighter bounds so cold-launch I/O stays small and
/// devices with less storage don't see Podcastr ballooning into hundreds
/// of MB just for cover art.
///
/// Tunables are deliberately conservative:
///   - Memory: 100 MB. Comfortably holds ~700 downsampled 64pt artwork
///     thumbnails (the Discover + Library + Today flow) plus a couple of
///     full-res 600×600 hero images (~1.5 MB each).
///   - Disk: 500 MB. Roughly 2,500 full-res 600×600 covers — more than
///     any user is likely to subscribe to.
///   - TTL: 14 days. Re-validates artwork every fortnight so a show that
///     swaps cover art is picked up within a sprint.
enum KingfisherConfiguration {

    private static let memoryByteLimit: UInt = 100 * 1024 * 1024  // 100 MB
    private static let diskByteLimit: UInt = 500 * 1024 * 1024    // 500 MB

    /// Apply bounds to the shared `ImageCache`. Idempotent — safe to call
    /// from both `AppDelegate.didFinishLaunching` and any future test
    /// harness that wants the same cache shape.
    static func configure() {
        let cache = ImageCache.default
        cache.memoryStorage.config.totalCostLimit = Int(memoryByteLimit)
        cache.diskStorage.config.sizeLimit = diskByteLimit
        cache.diskStorage.config.expiration = .days(14)
    }
}
