# Feature Reference

> **Historical template/feature research.** This document contains removed and
> superseded friend, Nostr, persistence, and agent examples. It is retained as
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

win-the-day has two agent channels: `.ownerChat` (voice/typed compose) and `.peerAgent` (Nostr inbound). The peer agent channel gets a different tool set — `send_friend_message` and `end_conversation` are only callable by peer agents, not the owner. Implement this pattern by adding a `channel` parameter to `AgentSession` and filtering `AgentTools.schema` based on it.

---

## Friends System

**Source:** win-the-day-app `RockingLife/Domain/Models.swift` (`NostrFriend`), `Features/Settings/AgentFriendsView.swift`

### Model

```swift
struct Friend: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var displayName: String
    var identifier: String   // Nostr pubkey, username, etc.
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

### Advanced: Nostr-backed friends (from win-the-day)

Replace `identifier` with a Nostr hex pubkey. Friends map to `nostrAllowedPubkeys` — incoming Nostr events from friends are auto-approved; from strangers they go to `nostrPendingApprovals`. See:
- `NostrAgentService.handleInbound()` — inbound event routing
- `NostrApprovalPresenter` — approval UI
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

## NIP-74 — Agent-Owned Podcasts

**Source:** `App/Sources/Agent/AgentTools+OwnedPodcasts.swift`, `App/Sources/Agent/LiveAgentOwnedPodcastManager.swift`, `App/Sources/Agent/AgentToolSchema+Podcast.swift`, `App/Sources/Features/Settings/Agent/AgentPodcastsView.swift`

### Concept

The AI agent can create and manage its own podcast shows — complete with AI-generated cover art and optional Nostr publishing via NIP-74. Agent-owned shows appear in the library alongside subscribed shows and can contain TTS-generated episodes.

### Tools

| Tool | Description |
|------|-------------|
| `create_podcast` | Create a new agent-owned show. Accepts `title`, `description`, `author`, `image_url`, `language`, `categories`, `visibility` (`public`/`private`). |
| `update_podcast` | Update metadata on an existing agent-owned show by `podcast_id`. |
| `delete_my_podcast` | Delete an agent-owned show and all its episodes. |
| `list_my_podcasts` | List all agent-owned shows with metadata and episode counts. |
| `generate_podcast_artwork` | Generate cover art via the configured image-gen model, upload via Blossom, return a CDN URL. |
| `publish_episode` | Publish an existing TTS episode to Nostr (NIP-74). Requires `visibility=public` and Nostr enabled in Settings. |

`generate_tts_episode` (in `AgentTools+TTS.swift`) accepts an optional `podcast_id`; when the podcast is public and Nostr is enabled, the episode is auto-published after generation.

### Lifecycle (`LiveAgentOwnedPodcastManager`)

1. `createPodcast(...)` — creates a `Podcast` with `kind: .synthetic` and calls `store.upsertPodcast(...)`.
2. If `visibility == .public` and Nostr is enabled, immediately publishes a NIP-74 show event signed by the agent's Nostr key from `NostrCredentialStore`.
3. `generateAndUploadArtwork(prompt:)` — calls the image-gen API (model configured in Image Generation Settings), uploads the result to the user's Blossom server, returns the CDN URL.
4. `publishEpisodeToNostr(episodeID:)` — looks up the episode, builds a NIP-74 episode event, signs and publishes, returns the `naddr`.

### Visibility

- `private` — show exists only in the local library; not signed or published.
- `public` — show and episodes are signed with the agent's nsec and published to Nostr relays as NIP-74 events.

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
