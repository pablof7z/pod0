# External Integrations

**Analysis Date:** 2026-08-22

## APIs & External Services

**LLM Provider:**
- OpenRouter - AI model orchestration and tool-calling agent backend
  - SDK/Client: Custom via reqwest HTTP client
  - Auth: API key stored in iOS Keychain (provider settings)
  - Endpoint: OpenAI-compatible `http://address/v1` format
  - Integration: `App/Sources/Features/Agent/AgentLLMClient.swift` handles provider configuration and requests

**Text-to-Speech:**
- ElevenLabs - TTS generation for podcast episodes and agent responses
  - SDK/Client: `pod0-tts-host` crate with custom HTTP implementation
  - Auth: API key via `ElevenLabsEndpoint` configuration
  - Endpoint: ElevenLabs-compatible proxy or direct endpoint (configurable)
  - Location: `rust/crates/pod0-tts-host/src/` - Self-contained TTS generation with error handling and streaming support

**Podcast Search/Discovery:**
- Apple Podcast Search API (implied via test fixtures)
  - Integration: Tests reference iTunes-compatible API format with track metadata
  - Endpoint: Configurable via `POD0_PODCAST_SEARCH_URL` environment variable

**Decentralized Protocol:**
- Nostr (decentralized event protocol) - For podcaster identity and content discovery
  - SDK/Client: `nostr` crate v0.44.6 with async WebSocket relay communication
  - Protocol: Nostr event protocol with cryptographic signing
  - Integration: `pod0-nostr-host` crate handles relay connections, event construction, and authentication
  - Location: `rust/crates/pod0-nostr-host/src/` - WebSocket relay communication, event signing, subscription management

## Data Storage

**Databases:**
- SQLite (bundled via rusqlite 0.39.0)
  - Client: Rusqlite with backup and bundled SQLite features
  - Schema: Versioned migrations with JSON metadata snapshots, workflow jobs, artifact metadata
  - Storage: `pod0.sqlite` file (location depends on host - iOS app documents, CLI working directory)
  - Extensions: sqlite-vec for vector similarity search via embedding indices
  - Location: Managed by `pod0-storage` crate (`rust/crates/pod0-storage/src/`)

**File Storage:**
- Local filesystem only
  - iOS: App documents directory via native URLSession downloads and temporary file handling
  - CLI: Working directory and configurable paths
  - Format: SQLite database file, WAV files (via hound), temporary download files

**Caching:**
- None detected - Transient in-memory caching via Rust data structures and iOS AppStateStore

## Authentication & Identity

**Auth Provider:**
- Custom per-provider basis
  - Keychain storage: iOS Keychain stores OpenRouter API keys, TTS provider secrets, and potential future auth tokens
  - Location: `App/Sources/Services/KeychainStore.swift` - Synchronous Keychain wrapper for Generic Password items
  - Security model: `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` (device-bound, never migrated)

**Nostr Identity:**
- Self-signed keypairs (ECDSA k256)
  - Generation: Via `pod0-nostr-host` using k256 with schnorr signing
  - Storage: Keychain (via Rust `pod0-system-hosts` keyring integration)
  - Location: `rust/crates/pod0-system-hosts/src/` - Platform-native keyring stores

## Monitoring & Observability

**Error Tracking:**
- None detected in current stack

**Logs:**
- Print/console logging via Rust `log` crate (max level: info) in `pod0-nostr-host`
- iOS: Native os.log framework (not visible in Rust bindings)
- CLI: Console output via println/eprintln

## CI/CD & Deployment

**Hosting:**
- iOS App Store / TestFlight (via GitHub Actions)
  - CI workflow: `.github/workflows/testflight.yml` - Builds and ships to TestFlight
  - Test workflow: `.github/workflows/test.yml` - Runs Rust tests and iOS unit/integration tests

**CI Pipeline:**
- GitHub Actions
  - Rust: Cargo test with all features
  - Swift: Xcode test with simulator
  - Dependency auditing: `cargo deny` for license and advisory checks

## Environment Configuration

**Required env vars:**
- `POD0_PODCAST_SEARCH_URL` - Podcast search endpoint (optional, used in tests)

**Secrets location:**
- iOS Keychain: OpenRouter API keys, ElevenLabs endpoint/key, Nostr private keys
  - Access: Synchronous Keychain queries via `KeychainStore` enum
  - Persistence: Device-local only, no backup/sync

**Configuration files:**
- `rust/Cargo.toml` - Workspace dependencies including API client versions
- `rust/uniffi.toml` - Language binding generation config
- Swift: Settings stored via iOS Keychain + AppStateStore (SwiftUI persistence)

## Webhooks & Callbacks

**Incoming:**
- None detected

**Outgoing:**
- RSS feed fetching (passive, pull-based)
- Podcast metadata API calls (pull-based)
- LLM API requests (request-response)
- TTS generation requests (request-response)
- Nostr relay connections (full duplex WebSocket)

## Data Flow & Integration Points

**Request Path: Podcast Fetch and Transcript Generation**
1. iOS app initiates podcast/feed fetch via native URLSession → `CoreDownloadHost`
2. Feed XML parsed via `quick-xml` in `pod0-application`
3. Metadata stored in SQLite via `pod0-storage`
4. User requests transcript: Optional provider selection (via Keychain-stored keys)
5. Transcript generation job queued in workflow system

**Request Path: Agent/TTS Interaction**
1. User voice input captured via native AVFoundation
2. Rust agent processes via `pod0-facade` → `pod0-application`
3. Tool calls formatted for OpenRouter LLM API (via reqwest)
4. OpenRouter response processed, TTS generation triggered
5. ElevenLabs TTS called via `pod0-tts-host` (custom HTTP implementation)
6. Audio stored in SQLite or returned via URLSession

**Request Path: Nostr Integration**
1. Podcast identity/content discovery via Nostr protocol
2. `pod0-nostr-host` initiates WebSocket relay connection (tokio-tungstenite)
3. Events signed with k256 ECDSA keypair (from Keychain)
4. Relay message protocol handled via nostr crate JSON serialization
5. Events indexed in SQLite recall index

## Platform-Specific Integrations

**iOS Native:**
- AVFoundation: Playback, session/route management, media controls
- URLSession: HTTP requests (feeds, APIs, downloads)
- Keychain: Secret storage (API keys, private keys)
- SwiftUI: UI framework integration
- User Notifications: Status and reminder notifications via `CoreNotificationHost`
- Media Remote Commands: Native playback controls

**macOS/Linux/Windows:**
- Native keyring stores (apple-native-keyring-store, zbus-secret-service, windows-native-keyring)
- Platform notifications (mac-usernotifications, notify-rust)
- Capability-based permissions (cap-std, cap-primitives)

---

*Integration audit: 2026-08-22*
