import Foundation
import UserNotifications
import os.log

// MARK: - Notification capability — `nmp.notification.capability`
//
// iOS half of the notification capability defined in
// `apps/nmp-app-podcast/src/capability/notification.rs` (feature #20). The
// capability translates `NotificationCommand` JSON into local notifications
// scheduled with `UNUserNotificationCenter`.
//
// Doctrine:
//   D0 — Rust decides *whether* to notify (today: every newly-discovered
//        episode on every refresh). This file never inspects payload content
//        to make that decision.
//   D6 — errors never throw across the boundary. Authorization denial,
//        scheduling errors, malformed payloads all degrade to a populated
//        error envelope or a silent no-op.
//   D7 — this capability *executes and reports*; there is no back-channel
//        report shape because the user-facing outcome (a notification banner)
//        is observable in the OS, not in Rust state.
//
// File-length budget: this file deliberately holds the whole capability
// (wire vocabulary + lifecycle + scheduling) — at ~180 LOC it is well under
// the 300-LOC soft limit.

/// Local-notification executor for the new-episode notification capability.
///
/// Single-instance, owned by `PodcastCapabilities`. Authorization is requested
/// once on `start()`; the request is non-blocking (wrapped in `Task`) so the
/// app launch path never stalls on the OS prompt.
@MainActor
final class NotificationCapability {
    static let namespace = "nmp.notification.capability"

    private let logger = Logger(subsystem: "io.f7z.podcast", category: "NotificationCapability")
    private let center: UNUserNotificationCenter
    private var started: Bool = false

    init(center: UNUserNotificationCenter = .current()) {
        self.center = center
    }

    // MARK: Lifecycle

    /// Idempotent. Marks the executor active and kicks off authorization.
    /// Safe to call on every app foreground; the OS deduplicates repeated
    /// `requestAuthorization` calls.
    func start() {
        guard !started else { return }
        started = true
        // Authorization is async; the OS prompt and the user's tap take
        // arbitrarily long. Detach so app start is not blocked.
        Task { [center, logger] in
            do {
                _ = try await center.requestAuthorization(options: [.alert, .sound, .badge])
            } catch {
                // D6: a denied prompt is data, not an exception. We log at
                // info because this is the expected fallthrough on a user
                // who has tapped "Don't Allow".
                logger.info("notification authorization request failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    /// Idempotent. Marks the executor inactive. Does not clear pending
    /// notifications — those are owned by the OS and survive process exit.
    func stop() {
        started = false
    }

    var isStarted: Bool { started }

    // MARK: - Command entry points

    /// Decode a `CapabilityRequest` JSON envelope and execute the contained
    /// `NotificationCommand`. Honours D6: malformed input degrades to an
    /// error envelope, never throws.
    @discardableResult
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            return errorEnvelope(correlationID: "", message: "malformed-request")
        }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let command = try? JSONDecoder().decode(NotificationCommand.self, from: payload)
        else {
            return errorEnvelope(correlationID: request.correlationID, message: "malformed-payload")
        }
        execute(command)
        return okEnvelope(correlationID: request.correlationID)
    }

    /// Direct command entry — used by tests and any future synchronous
    /// caller. Pure translation; no policy decisions.
    func execute(_ command: NotificationCommand) {
        switch command {
        case let .scheduleNewEpisode(episodeTitle, podcastTitle, episodeID):
            scheduleNewEpisode(
                episodeTitle: episodeTitle,
                podcastTitle: podcastTitle,
                episodeID: episodeID)
        }
    }

    // MARK: - Command implementations

    private func scheduleNewEpisode(
        episodeTitle: String,
        podcastTitle: String,
        episodeID: String
    ) {
        let content = UNMutableNotificationContent()
        content.title = podcastTitle
        content.body = "New episode: \(episodeTitle)"
        content.sound = .default
        // userInfo lets a future deep-link / tap handler route straight to
        // the episode without re-parsing the title or body strings.
        content.userInfo = ["episodeId": episodeID]

        // A 1-second trigger (rather than nil) keeps the scheduling path
        // identical for tests and for the live capability: the OS still
        // raises a banner, the request can be inspected via
        // `getPendingNotificationRequests` in the brief interval before it
        // fires, and `repeats: false` ensures it fires exactly once.
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false)

        let request = UNNotificationRequest(
            // The `episodeID` is unique per episode, so using it as the
            // request identifier means a duplicate `scheduleNewEpisode` for
            // the same episode replaces the pending notification rather
            // than queueing a second one — a free dedupe across rapid
            // back-to-back refreshes.
            identifier: "new-episode.\(episodeID)",
            content: content,
            trigger: trigger)

        center.add(request) { [logger] error in
            if let error {
                // D6: a scheduling failure is data, not an exception.
                logger.error("notification scheduling failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    // MARK: - Envelope encoding

    private func okEnvelope(correlationID: String) -> String {
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: "{\"status\":\"ok\"}")
        return Self.encodeEnvelope(env) ?? "{}"
    }

    private func errorEnvelope(correlationID: String, message: String) -> String {
        let payload = "{\"status\":\"error\",\"message\":\"\(message)\"}"
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: payload)
        return Self.encodeEnvelope(env) ?? "{}"
    }

    private static func encodeEnvelope<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

// MARK: - Notification capability wire vocabulary
//
// Swift mirror of the Rust types in
// `apps/nmp-app-podcast/src/capability/notification.rs`. The Rust enum is
// `#[serde(tag = "type", rename_all = "snake_case")]`; the manual `Codable`
// impl below matches that wire shape exactly so a JSON string produced on
// one side decodes on the other.

/// Commands Rust dispatches to the iOS notification executor.
///
/// Wire shape (Rust side, `serde` tagged on `"type"`, snake_case):
///
/// ```text
/// {"type":"schedule_new_episode","episode_title":"…","podcast_title":"…","episode_id":"…"}
/// ```
enum NotificationCommand: Decodable, Equatable {
    case scheduleNewEpisode(episodeTitle: String, podcastTitle: String, episodeID: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case episodeTitle = "episode_title"
        case podcastTitle = "podcast_title"
        case episodeID = "episode_id"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "schedule_new_episode":
            self = .scheduleNewEpisode(
                episodeTitle: try c.decode(String.self, forKey: .episodeTitle),
                podcastTitle: try c.decode(String.self, forKey: .podcastTitle),
                episodeID: try c.decode(String.self, forKey: .episodeID))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown notification command: \(type)")
        }
    }
}
