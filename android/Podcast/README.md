# Podcast - Android second-platform client

Compose project that drives the same `nmp-app-podcast` Rust crate the iOS app
links. Android renders snapshots and executes OS capabilities; podcast policy,
state transitions, feed/search behavior, playback policy, and download queue
ownership stay in the Rust kernel.

## What this proves

- **Same Rust app kernel, second platform.** `apps/nmp-app-podcast/Cargo.toml`
  builds a `cdylib` so cargo-ndk can pack `libnmp_app_podcast.so` for
  `arm64-v8a` and `x86_64`.
- **Generated UniFFI is the app boundary.** `KernelBridge.kt` owns a generated
  `PodcastApp`. Runtime lifecycle, update/capability callbacks, identity,
  NIP-46, NIP-55, ref resolution, snapshot decode, action dispatch, and
  app-domain bridge calls flow through generated UniFFI instead of handwritten
  JNI entry points.
- **Android remains a capability shell.** Android routes HTTP, audio, download,
  and external-signer work back to Rust as typed capability reports. Rust
  decides state changes; Kotlin executes platform effects and renders state.

## Layout

```text
android/Podcast/
|-- README.md
|-- build.gradle.kts
|-- settings.gradle.kts
|-- gradle.properties
`-- app/
    |-- build.gradle.kts
    `-- src/main/
        |-- AndroidManifest.xml
        |-- jniLibs/{arm64-v8a,x86_64}/
        `-- java/
            |-- io/f7z/podcast/KernelBridge.kt
            |-- io/f7z/podcast/MainActivity.kt
            |-- io/f7z/podcast/PodcastSnapshot.kt
            `-- uniffi/nmp_app_podcast/nmp_app_podcast.kt
```

## Prerequisites

| Tool | Version | How |
|---|---|---|
| Android SDK | 34 | `~/Library/Android/sdk` or Android Studio |
| Android NDK | 26.1+ | `sdkmanager --install "ndk;26.1.10909125"` |
| Rust | 1.78+ | `rustup` |
| Rust targets | `aarch64-linux-android`, `x86_64-linux-android` | `rustup target add ...` |
| cargo-ndk | 3.5+ | `cargo install cargo-ndk` |
| JDK | 17 | `brew install openjdk@17` |

Set `ANDROID_NDK_HOME` if Gradle/cargo-ndk cannot infer it:

```sh
export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/26.1.10909125
```

## Build

From the repo root, the manual Rust step is:

```sh
cargo ndk \
  --manifest-path apps/nmp-app-podcast/Cargo.toml \
  -t arm64-v8a -t x86_64 \
  -o android/Podcast/app/src/main/jniLibs \
  build --release
```

The normal Android path runs that through Gradle:

```sh
cd android/Podcast
./gradlew assembleDebug
```

`app/build.gradle.kts` wires `cargoNdk` into `preBuild`, so Gradle rebuilds the
Rust shared libraries before compiling the Kotlin app.

## Current status

| Gate | Status |
|---|---|
| `nmp-app-podcast` builds as Android `cdylib` | Done |
| Generated UniFFI Kotlin binding is checked in | Done |
| `KernelBridge.kt` uses generated `PodcastApp` instead of handwritten JNI | Done |
| Compose shell decodes and renders the Rust snapshot model | Done |
| Subscribe/search/feed refresh execute through Rust-owned actions/capabilities | Done |
| ExoPlayer commands and audio reports round-trip through Rust | Done |
| Download UI and OkHttp executor report progress to Rust | Done |
| Provider, catalog, transcript, image, rerank, and BYOK bridge calls use UniFFI | Done |

Remaining parity work is product behavior, not an Android-specific FFI surface:
lock-screen command policy validation, key-management UX, and later AI/Nostr
platform integrations.

## Doctrine reminders

- Kotlin renders, dispatches actions, and executes OS capabilities. Podcast
  domain logic belongs in Rust.
- Kotlin stores only ephemeral presentation or queue state needed to bridge OS
  callbacks.
- External effects are reported back to Rust. Rust owns the next state.
