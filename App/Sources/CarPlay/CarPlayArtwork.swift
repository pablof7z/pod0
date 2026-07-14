import CarPlay
import Foundation
import Kingfisher
import UIKit

// MARK: - CarPlayArtwork
//
// Tiny helper that fetches an artwork URL through the shared Kingfisher cache
// and resizes the resulting `UIImage` to `CPListItem.maximumImageSize` so
// CarPlay accepts it. CarPlay rejects oversize art (mid-2020s head units
// enforce roughly 60-110pt images, exact size in `maximumImageSize`).
//
// Results are delivered on the main actor. Callers are expected to keep a
// reference to the `CPListItem` and assign via `setImage(_:)` when the fetch
// lands — CarPlay updates the row in place without re-pushing the template.

@MainActor
enum CarPlayArtwork {

    /// Fetch `url` through Kingfisher, resize to CarPlay's accepted bounds,
    /// hand the result back on the main actor. Calls `completion(nil)` on
    /// failure so the caller can leave the placeholder in place.
    static func fetch(_ url: URL?, completion: @escaping @MainActor (UIImage?) -> Void) {
        guard let url else { completion(nil); return }
        let bounds = CPListItem.maximumImageSize
        KingfisherManager.shared.retrieveImage(with: url) { result in
            switch result {
            case .success(let value):
                let resized = resize(value.image, to: bounds)
                Task { @MainActor in completion(resized) }
            case .failure:
                Task { @MainActor in completion(nil) }
            }
        }
    }

    /// Synchronously resize a UIImage with `UIGraphicsImageRenderer`. The
    /// inner drawing closure is `@Sendable` so we can hop off-main if we
    /// ever wire this through a background queue.
    nonisolated static func resize(_ image: UIImage, to size: CGSize) -> UIImage {
        let renderer = UIGraphicsImageRenderer(size: size)
        return renderer.image { @Sendable _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }
}
