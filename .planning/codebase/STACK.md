# Technology Stack

**Analysis Date:** 2026-08-22

## Languages

**Primary:**
- Rust 1.93.0 - Core domain, application logic, storage, recall indexing, facades, and CLI
- Swift (UIKit/SwiftUI) - iOS native frontend, AVFoundation audio layer, platform integration
- Kotlin - Generated bindings via UniFFI (not currently active in build)

**Secondary:**
- YAML - GitHub Actions CI/CD workflows
- Shell - Build scripts and utilities

## Runtime

**Environment:**
- Rust: Tokio async runtime 1.53.1 with multi-threaded features
- iOS: Swift concurrency (structured concurrency model)
- SQLite: Bundled SQLite via rusqlite, accessed through connection pooling

**Package Manager:**
- Rust: Cargo (workspace-based)
  - Lockfile: `rust/Cargo.lock` (present)
  - Edition: 2024
- Swift: Xcode 26.6 with SwiftUI framework
- iOS build: Tuist 4.200.5 for project generation

## Frameworks

**Core:**
- UniFFI 0.32.0 - Rust-to-Swift/Kotlin foreign function interface with generated bindings
- AVFoundation - iOS native audio playback, session management, media controls
- SwiftUI - iOS native UI framework
- Tokio 1.53.1 - Async runtime with `rt`, `time` features; expanded features in specific crates

**Web/HTTP:**
- Reqwest 0.12.28 - HTTP client with `blocking`, `json`, `rustls-tls` features
- Tungstenite 0.29.0 - WebSocket support
- Tokio-tungstenite 0.29.0 - Async WebSocket integration

**Storage/Persistence:**
- Rusqlite 0.39.0 - SQLite bindings with `backup` and `bundled` SQLite
- SQLite-vec 0.1.9 - Vector storage extension for similarity search

**Serialization:**
- Serde 1.0.228 - Serialization framework with derive macros
- Serde_json 1.0.150 - JSON encoding/decoding
- Askama 0.16.0 - Template rendering engine with serde integration
- Quick-xml 0.41.0 - XML parsing (for RSS feed handling)

**Audio/Media:**
- Rodio 0.21.1 - Audio playback library supporting flac, mp3, mp4, vorbis, wav
- Hound 3.5.1 - WAV file I/O
- macOS-specific: Rodio with `playback` feature enabled

**Security & Cryptography:**
- SHA2 0.10.9 - SHA-256 hashing
- K256 0.13.4 - ECDSA cryptography (arithmetic, schnorr, std features)
- Zeroize 1.9.0 - Secure memory clearing
- Keyring-core 1.0.0 - Native keyring access (macOS, Windows, Linux)
- Apple-native-keyring-store 1.0.2 - macOS Keychain integration

**Protocol Support:**
- URL 2.5.7 - URL parsing and manipulation

**Testing & Utilities:**
- Tempfile 3.27.0 - Temporary file handling
- Time 0.3.47 - Date/time with formatting and parsing
- Regex 1.13.1 - Regular expressions
- Unicode-segmentation 1.12.0 - Unicode grapheme handling
- Libc 0.2.186 - C standard library bindings

**Platform-Specific Dependencies:**
- macOS: `apple-native-keyring-store`, `mac-usernotifications`, `objc2-foundation`
- Windows: `windows-native-keyring-store`, `notify-rust`
- Linux: `zbus-secret-service-keyring-store`, `notify-rust`
- All: `cap-std`, `cap-primitives` (capability-based permissions) for macOS/Linux

## Configuration

**Environment:**
- Rust toolchain: `rust/rust-toolchain.toml` - Channel 1.93.0, minimal profile, clippy and rustfmt components
- UniFFI configuration: `rust/uniffi.toml` - Swift and Kotlin code generation with immutable records
- Dependency audit: `rust/deny.toml` - License checking, advisory scanning, allowed registries

**Build:**
- Cargo workspace: `rust/Cargo.toml` with 10 member crates
- Xcode project: `Podcastr.xcodeproj` (generated via Tuist)
- iOS minimum deployment target: iOS 26 (beta)

**API/Environment Variables:**
- `POD0_PODCAST_SEARCH_URL` - Podcast search endpoint (tests reference local HTTP endpoint)
- OpenAI-compatible API endpoint format: `http://address/v1`
- ElevenLabs TTS endpoint configuration via `ElevenLabsEndpoint` struct
- Provider secrets stored in iOS Keychain (OpenRouter API keys, TTS provider keys)

## Platform Requirements

**Development:**
- Xcode 26.6 (build 17F113) with iOS 26 simulator runtime
- Tuist 4.200.5 (locked in `.tool-versions`)
- Rust 1.93.0 via `rust-toolchain.toml`
- macOS development machine

**Production:**
- iOS 26+ (deployment target)
- SQLite database (bundled)
- Native capabilities: AVFoundation audio, URLSession networking, Keychain secrets

## Key Dependencies Map

| Crate | Purpose | Key Dependencies |
|-------|---------|------------------|
| `pod0-domain` | Core domain models | sha2, serde, uniffi |
| `pod0-application` | Application logic | pod0-domain, regex, quick-xml, time, url, serde, unicode-segmentation |
| `pod0-storage` | Persistent storage | rusqlite, serde, sha2, url, time |
| `pod0-recall-index` | Vector similarity search | rusqlite, sqlite-vec, serde, sha2 |
| `pod0-facade` | UniFFI facade for iOS | uniffi, sha2, all core crates |
| `pod0-cli` | Command-line interface | tokio, reqwest, rustyline, all core crates |
| `pod0-portable-media` | Audio playback/streaming | rodio, hound, reqwest, tokio, url |
| `pod0-live-hosts` | Live capability adapters | tokio, reqwest, serde, zeroize |
| `pod0-system-hosts` | OS-level capabilities | keyring, notifications, cap-std/cap-primitives |
| `pod0-tts-host` | Text-to-speech generation | reqwest, serde, tokio, zeroize |

---

*Stack analysis: 2026-08-22*
