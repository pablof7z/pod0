import UIKit
import UniformTypeIdentifiers

@MainActor
final class ShareViewController: UIViewController {
    private let iconView = UIImageView()
    private let titleLabel = UILabel()
    private let detailLabel = UILabel()
    private let progress = UIActivityIndicatorView(style: .medium)
    private let closeButton = UIButton(type: .system)
    private var hasStarted = false
    private var hasFailed = false
    private var candidates: [(NSItemProvider, String)] = []

    override func viewDidLoad() {
        super.viewDidLoad()
        configureView()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !hasStarted else { return }
        hasStarted = true
        loadSharedURL()
    }

    private func configureView() {
        view.backgroundColor = .systemBackground
        preferredContentSize = CGSize(width: 360, height: 220)

        iconView.image = UIImage(systemName: "arrow.down.circle")
        iconView.tintColor = .label
        iconView.preferredSymbolConfiguration = UIImage.SymbolConfiguration(
            pointSize: 32,
            weight: .medium
        )
        iconView.contentMode = .scaleAspectFit

        titleLabel.text = "Add to Pod0"
        titleLabel.font = .systemFont(ofSize: 20, weight: .semibold)
        titleLabel.textAlignment = .center

        detailLabel.text = "Saving episode link…"
        detailLabel.font = .systemFont(ofSize: 15, weight: .regular)
        detailLabel.textColor = .secondaryLabel
        detailLabel.textAlignment = .center
        detailLabel.numberOfLines = 2

        progress.startAnimating()

        closeButton.setTitle("Cancel", for: .normal)
        closeButton.titleLabel?.font = .systemFont(ofSize: 15, weight: .medium)
        closeButton.addTarget(self, action: #selector(closeTapped), for: .touchUpInside)

        let stack = UIStackView(arrangedSubviews: [
            iconView,
            titleLabel,
            detailLabel,
            progress,
            closeButton
        ])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)

        NSLayoutConstraint.activate([
            iconView.widthAnchor.constraint(equalToConstant: 42),
            iconView.heightAnchor.constraint(equalToConstant: 42),
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -24),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor)
        ])
    }

    private func loadSharedURL() {
        let providers = extensionContext?.inputItems
            .compactMap { $0 as? NSExtensionItem }
            .flatMap { $0.attachments ?? [] }
            ?? []
        candidates = providers.flatMap { provider in
            [UTType.url.identifier, UTType.plainText.identifier]
                .filter { provider.hasItemConformingToTypeIdentifier($0) }
                .map { (provider, $0) }
        }
        loadCandidate(at: 0)
    }

    private func loadCandidate(at index: Int) {
        guard candidates.indices.contains(index) else {
            showFailure("Share a podcast episode link to add it to Pod0.")
            return
        }
        let (provider, typeIdentifier) = candidates[index]
        provider.loadItem(forTypeIdentifier: typeIdentifier) { [weak self] item, _ in
            let url = Self.webURL(from: item)
            Task { @MainActor in
                guard let self else { return }
                if let url {
                    self.enqueue(url)
                } else {
                    self.loadCandidate(at: index + 1)
                }
            }
        }
    }

    private func enqueue(_ url: URL) {
        do {
            let store = try SharedEpisodeImportRequestStore.appGroup()
            try store.enqueue(sourceURL: url)
            progress.stopAnimating()
            iconView.image = UIImage(systemName: "checkmark.circle.fill")
            iconView.tintColor = .systemGreen
            titleLabel.text = "Sent to Pod0"
            detailLabel.text = "Pod0 will add and download it when you return."
            closeButton.isHidden = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
                self?.extensionContext?.completeRequest(returningItems: nil)
            }
        } catch {
            showFailure(
                (error as? LocalizedError)?.errorDescription
                    ?? "Pod0 could not save this episode link."
            )
        }
    }

    private func showFailure(_ message: String) {
        hasFailed = true
        progress.stopAnimating()
        iconView.image = UIImage(systemName: "exclamationmark.circle")
        iconView.tintColor = .systemRed
        titleLabel.text = "Couldn’t add episode"
        detailLabel.text = message
        closeButton.setTitle("Close", for: .normal)
    }

    @objc private func closeTapped() {
        if hasFailed {
            extensionContext?.completeRequest(returningItems: nil)
        } else {
            extensionContext?.cancelRequest(withError: CocoaError(.userCancelled))
        }
    }

    nonisolated private static func webURL(from item: NSSecureCoding?) -> URL? {
        let candidate: URL?
        switch item {
        case let url as URL:
            candidate = url
        case let url as NSURL:
            candidate = url as URL
        case let text as String:
            candidate = firstWebURL(in: text)
        case let text as NSString:
            candidate = firstWebURL(in: text as String)
        default:
            candidate = nil
        }
        guard let candidate,
              ["http", "https"].contains(candidate.scheme?.lowercased() ?? "")
        else { return nil }
        return candidate
    }

    nonisolated private static func firstWebURL(in text: String) -> URL? {
        if let direct = URL(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
           direct.scheme != nil {
            return direct
        }
        guard let detector = try? NSDataDetector(
            types: NSTextCheckingResult.CheckingType.link.rawValue
        ) else { return nil }
        return detector.firstMatch(
            in: text,
            range: NSRange(text.startIndex..., in: text)
        )?.url
    }
}
