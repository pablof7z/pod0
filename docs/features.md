# Feature Reference

> **Historical template/feature research.** This document contains removed and
> provenance, not as a current implementation guide. Use
> [`architecture.md`](architecture.md), the
> [architecture ADRs](architecture/README.md), and code/tests on `master`.

## Agent System

**Source:** win-the-day-app `RockingLife/Agent/AgentSession.swift`, `AgentPrompt.swift`, `Agent/Tools.swift`

### Loop mechanics

```
user utterance
    → build messages[] with system prompt + user message
    → read OpenRouter credential from Keychain
    → call OpenRouter /chat/completions with tools schema
    → parse response
    → if toolCalls present:
          dispatch each tool → get JSON result
          append result as role:tool message
          loop (up to maxTurns)
    → if no toolCalls:
          done (final assistant text)
```

The agent receives a rich system prompt built from live `AppState`:
- Current pending items (with IDs for targeting)
- Friends list (with IDs for peer attribution)
- Agent memories (persisted facts about the user)

OpenRouter credentials are connected in Settings through BYOK (`key:openrouter`) or saved manually. Raw provider keys are stored in Keychain, not in the JSON app-state blob.

### Tool dispatch

Tools return JSON strings (`{"success": true, "id": "..."}` or `{"error": "..."`}). The JSON is fed back as `role: tool` messages so the model sees the result.

### Adding tools

1. Add entry to `AgentTools.schema` (OpenAI function format)
2. Add `case "tool_name":` in `AgentTools.dispatch`
3. Call the appropriate `AppStateStore` method
4. Return `success(...)` or `error(...)`

### Channel concept (from win-the-day)


---

## Friends System


### Model

```swift
struct Friend: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var displayName: String
    var addedAt: Date
    var avatarURL: String?
    var about: String?
}
```

### Peer attribution

When a friend's agent creates items, tag them:
```swift
store.addItem(title: title, source: .agent, friendID: friend.id, friendName: friend.displayName)
```

The `HomeView.ItemRow` reads `requestedByDisplayName` to display "From Alice" under the task.


- `AgentFriendsView` — QR code add, relay management

---

## Anchor System

**Source:** win-the-day-app `RockingLife/Domain/Models.swift`

### Pattern

A polymorphic `enum` with associated values, serialized as `{ "kind": "...", "id/date/..." }`.

```swift
enum Anchor: Codable, Hashable, Sendable {
    case item(id: UUID)
    case note(id: UUID)
    // Extend:
    case thread(id: UUID)
    case day(date: String)   // "2026-05-04"
    case week(weekStart: String)
}
```

Notes target an anchor. The agent can create notes about specific items:
```swift
store.addNote(text: "This task is blocked", target: .item(id: taskID))
```

### Queries

```swift
// Notes about a specific item:
let notes = store.activeNotes.filter { $0.target == .item(id: someID) }
```

---

## Persistence

**Source:** win-the-day-app `RockingLife/State/Persistence.swift`

### Strategy

Single JSON blob in **App Group UserDefaults** (`group.com.podcastr.app`). App Group is required to share state with widgets, watch extensions, or share extensions.

JSON uses ISO8601 dates and sorted keys for deterministic output (stable diffs).

### Extending

For iCloud sync, add `NSUbiquitousKeyValueStore` alongside the local save:
```swift
// After UserDefaults.set:
NSUbiquitousKeyValueStore.default.set(data, forKey: "podcastr.state.v1")
NSUbiquitousKeyValueStore.default.synchronize()
```

Observe external changes:
```swift
NotificationCenter.default.addObserver(forName: NSUbiquitousKeyValueStore.didChangeExternallyNotification, ...) { _ in
    // Merge cloud state into local
}
```

For SwiftData (used in cut-tracker), replace the JSON blob with a `ModelContainer` and SwiftData `@Model` classes.

---


**Source:** `App/Sources/Agent/AgentTools+OwnedPodcasts.swift`, `App/Sources/Agent/LiveAgentOwnedPodcastManager.swift`, `App/Sources/Agent/AgentToolSchema+Podcast.swift`, `App/Sources/Features/Settings/Agent/AgentPodcastsView.swift`

### Concept


### Tools

| Tool | Description |
|------|-------------|
| `create_podcast` | Create a new agent-owned show. Accepts `title`, `description`, `author`, `image_url`, `language`, `categories`, `visibility` (`public`/`private`). |
| `update_podcast` | Update metadata on an existing agent-owned show by `podcast_id`. |
| `delete_my_podcast` | Delete an agent-owned show and all its episodes. |
| `list_my_podcasts` | List all agent-owned shows with metadata and episode counts. |


### Lifecycle (`LiveAgentOwnedPodcastManager`)

1. `createPodcast(...)` — sends a typed synthetic-podcast input through the Pod0 Rust facade, which commits the show to the shared library.

### Visibility

- `private` — show exists only in the local library; not signed or published.

Visibility can be changed after creation via `update_podcast(podcast_id:, visibility:)`.

### Settings

**Settings → Agent → Podcasts** (`AgentPodcastsView`) lists all agent-owned shows with their visibility status and episode counts, and links to Image Generation Settings for cover art configuration.

---

## CI/CD Pipeline

**Source:** win-the-day-app `ci_scripts/`, `.github/workflows/`

### Key difference from win-the-day

win-the-day uses XcodeGen (`xcodegen generate`). Podcastr uses Tuist (`tuist generate --no-open`). The rest of the CI pipeline is similar.

### Version numbering

`archive_and_upload.sh` reads `CFBundleShortVersionString` from the app `Info.plist` for the marketing version, applies the same marketing/build values to the app and widget plists, and verifies the archived app/widget metadata before export. Build number is a UTC timestamp (`YYYYMMDDHHmm`) — unique per submission, monotonically increasing, requires no manual bump.

### Signing modes

- **Automatic** (default): Xcode manages profiles. Works when runner has Apple Developer account in Xcode.
- **Manual**: Triggered when `APPLE_DISTRIBUTION_CERTIFICATE_BASE64` secret is set. Creates a temporary keychain with the certificate, then passes explicit `CODE_SIGN_IDENTITY=Apple Distribution` plus app/widget profile specifiers to xcodebuild.

The manual mode workaround exists because Xcode 26 beta has a bug where automatic provisioning auth always fails in CI. Manual signing avoids the auth entirely.
