import SwiftUI

// MARK: - FeedbackCategory

enum FeedbackCategory: String, Codable, CaseIterable, Identifiable {
    case bug = "Bug"
    case featureRequest = "Feature Request"
    case question = "Question"
    case praise = "Praise"

    var id: String { rawValue }

    var tagValue: String {
        switch self {
        case .bug: "bug"
        case .featureRequest: "feature-request"
        case .question: "question"
        case .praise: "praise"
        }
    }

    static func from(tags: [[String]]) -> FeedbackCategory {
        let tagged = tags.first { tag in
            tag.count >= 2 && (tag[0] == "t" || tag[0] == "category")
        }?[1].lowercased()
        guard let tagged else { return .bug }
        return Self.allCases.first {
            $0.tagValue == tagged || $0.rawValue.lowercased() == tagged
        } ?? .bug
    }

    var icon: String {
        switch self {
        case .bug: "ant.fill"
        case .featureRequest: "lightbulb.fill"
        case .question: "questionmark.circle.fill"
        case .praise: "heart.fill"
        }
    }

    var tint: Color {
        switch self {
        case .bug: .red
        case .featureRequest: .blue
        case .question: .purple
        case .praise: .pink
        }
    }
}

// MARK: - FeedbackThread

struct FeedbackThread: Identifiable {
    var id: String { eventID }
    var eventID: String
    var authorPubkey: String
    var category: FeedbackCategory
    var content: String
    var attachedImage: UIImage?
    var title: String?
    var summary: String?
    var statusLabel: String?
    var replies: [FeedbackReply] = []
    var createdAt: Date = Date()

    init(
        eventID: String = "local-\(UUID().uuidString)",
        authorPubkey: String = "",
        category: FeedbackCategory,
        content: String,
        attachedImage: UIImage? = nil,
        title: String? = nil,
        summary: String? = nil,
        statusLabel: String? = nil,
        replies: [FeedbackReply] = [],
        createdAt: Date = Date()
    ) {
        self.eventID = eventID
        self.authorPubkey = authorPubkey
        self.category = category
        self.content = content
        self.attachedImage = attachedImage
        self.title = title
        self.summary = summary
        self.statusLabel = statusLabel
        self.replies = replies
        self.createdAt = createdAt
    }

    init(
        event: SignedNostrEvent,
        replies: [SignedNostrEvent] = [],
        metadata: FeedbackMetadata? = nil,
        attachedImage: UIImage? = nil,
        localPubkey: String? = nil
    ) {
        self.eventID = event.id
        self.authorPubkey = event.pubkey
        self.category = FeedbackCategory.from(tags: event.tags)
        self.content = event.content
        self.attachedImage = attachedImage
        self.title = metadata?.title
        self.summary = metadata?.summary
        self.statusLabel = metadata?.statusLabel
        self.replies = replies.map { FeedbackReply(event: $0, localPubkey: localPubkey) }
        self.createdAt = Date(timeIntervalSince1970: TimeInterval(event.created_at))
    }
}

// MARK: - FeedbackMetadata

struct FeedbackMetadata {
    let createdAt: Int
    let title: String?
    let summary: String?
    let statusLabel: String?

    init(event: SignedNostrEvent) {
        createdAt = event.created_at

        var title: String?
        var summary: String?
        var status: String?
        for tag in event.tags where tag.count >= 2 {
            switch tag[0] {
            case "title":
                title = title ?? tag[1]
            case "summary":
                summary = summary ?? tag[1]
            case "status-label", "status_label", "status":
                status = status ?? tag[1]
            default:
                break
            }
        }

        if title == nil || summary == nil || status == nil,
           let data = event.content.data(using: .utf8),
           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            title = title ?? json["title"] as? String
            summary = summary ?? json["summary"] as? String
            status = status ?? json["status_label"] as? String ?? json["status"] as? String
        }

        self.title = title
        self.summary = summary
        self.statusLabel = status
    }
}

// MARK: - FeedbackReply

struct FeedbackReply: Identifiable {
    var id: String { eventID }
    var eventID: String
    var authorPubkey: String
    var content: String
    var isFromMe: Bool
    var createdAt: Date = Date()

    init(event: SignedNostrEvent, localPubkey: String?) {
        eventID = event.id
        authorPubkey = event.pubkey
        content = event.content
        isFromMe = event.pubkey == localPubkey
        createdAt = Date(timeIntervalSince1970: TimeInterval(event.created_at))
    }
}

// MARK: - Nostr feedback helpers

extension SignedNostrEvent {
    var projectATags: [String] {
        tags.compactMap { tag in
            tag.count >= 2 && tag[0] == "a" ? tag[1] : nil
        }
    }

    var eTagIDs: [String] {
        tags.compactMap { tag in
            tag.count >= 2 && tag[0] == "e" ? tag[1] : nil
        }
    }

    var rootEventID: String? {
        if let marked = tags.first(where: { tag in
            tag.count >= 4 && tag[0] == "e" && tag[3] == "root"
        }) {
            return marked[1]
        }
        return eTagIDs.first
    }
}
