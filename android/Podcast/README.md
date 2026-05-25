# Podcast — Android (M2.F second-platform proof)

Minimal Compose project that drives the **same** `nmp-app-podcast` Rust crate
the iOS app links. The point of this directory is to prove the kernel
composition is platform-portable, not to ship a production Android client.

## What this proves

- **Same cargo binary, second platform.** `apps/nmp-app-podcast/Cargo.toml`
  adds `cdylib` to `crate-type` alongside the existing `staticlib` / `rlib`.
  iOS keeps linking the `.a`; cargo-ndk packs the same crate as
  `libnmp_app_podcast.so` for `arm64-v8a` and `x86_64`.
- **JNI surface mirrors the iOS C ABI.**
  `apps/nmp-app-podcast/src/android.rs` (gated `#[cfg(target_os = "android")]`)
  exports `Java_io_f7z_podcast_KernelBridge_*` symbols that bind 1:1 to the
  `external fun` declarations in `KernelBridge.kt`. The surface is a faithful
  port of `ios/Podcast/Podcast/Bridge/KernelBridge.swift`.
- **One capability hop works.** `nativeSigninNsec` and `nativeDispatchAction`
  thread through to `nmp_app_signin_nsec` / `nmp_app_dispatch_action` in
  `nmp-ffi` — the exit-checklist gate for "one capability hop on the second
  platform".

## Layout

```
android/Podcast/
├── README.md                              -- this file
├── build.gradle.kts                       -- project-level (plugin versions)
├── settings.gradle.kts                    -- :app inclusion
├── gradle.properties                      -- jvmargs + AndroidX flag
├── .gitignore                             -- excludes build/, .gradle/, *.so
└── app/
    ├── build.gradle.kts                   -- module build + cargo-ndk task
    └── src/main/
        ├── AndroidManifest.xml
        ├── res/values/strings.xml
        ├── jniLibs/{arm64-v8a,x86_64}/    -- cargo-ndk drops .so files here
        └── java/io/f7z/podcast/
            ├── PodcastApp.kt              -- Application class
            ├── MainActivity.kt            -- single-activity Compose host
            ├── KernelBridge.kt            -- JNI wrapper (mirror of iOS)
            └── PodcastSnapshot.kt         -- @Serializable snapshot model
```

## Prerequisites

| Tool | Version | How |
|---|---|---|
| Android SDK | 34 | `~/Library/Android/sdk` (Android Studio) |
| Android NDK | 26.1+ | `sdkmanager --install "ndk;26.1.10909125"` |
| Rust | 1.78+ | `rustup` |
| Rust targets | `aarch64-linux-android`, `x86_64-linux-android` | `rustup target add ...` |
| cargo-ndk | 3.5+ | `cargo install cargo-ndk` |
| JDK | 17 | `brew install openjdk@17` |

Set `ANDROID_NDK_HOME` (or `ANDROID_HOME` + sub-path resolution by
cargo-ndk):

```sh
export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/26.1.10909125
```

## Build

Two equivalent paths.

### 1. Manual cargo-ndk + Gradle (current PoC flow)

From the **repo root**:

```sh
# Cross-compile the Rust crate for both ABIs into jniLibs/
cargo ndk \
  --manifest-path apps/nmp-app-podcast/Cargo.toml \
  -t arm64-v8a -t x86_64 \
  -o android/Podcast/app/src/main/jniLibs \
  build --release

# Then the Android build
cd android/Podcast
./gradlew assembleDebug
```

The Rust step in isolation is the **proof gate**: if it produces an
`.so` whose `Java_io_f7z_podcast_KernelBridge_*` symbols match the Kotlin
`external fun` declarations, the cross-platform FFI works. Verify with:

```sh
NDK_HOST=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64
$NDK_HOST/bin/llvm-nm -D \
  target/aarch64-linux-android/release/libnmp_app_podcast.so \
  | grep Java_io_f7z_podcast
```

You should see 13 `T Java_io_f7z_podcast_KernelBridge_<native>` entries
(11 from M2.F + `nmpActionDispatch` and `nmpCapabilityReport` from M13.A).

### 2. Gradle-driven (what `assembleDebug` does)

`app/build.gradle.kts` registers a `cargoNdk` task wired into `preBuild`,
so a plain `./gradlew assembleDebug` runs the cargo-ndk step automatically.
This is the production path.

## How the Rust lib is linked

1. `apps/nmp-app-podcast/Cargo.toml` declares `crate-type = ["staticlib",
   "rlib", "cdylib"]`. The `cdylib` is what cargo-ndk packs into
   `libnmp_app_podcast.so`.
2. `apps/nmp-app-podcast/src/android.rs` exports the `Java_*` JNI entry
   points behind `#[cfg(target_os = "android")]`. iOS sees no Kotlin/JNI
   types because the `jni` crate dep is itself
   `[target.'cfg(target_os = "android")']`-gated.
3. The JNI entry points call into the kernel through **Rust paths**
   (`nmp_ffi::nmp_app_new`, `nmp_ffi::nmp_app_signin_nsec`, etc.). Calling
   through `extern "C" {}` declarations is unreliable here: rustc only pulls
   rlib bodies into the cdylib when they are reachable through Rust names.
4. `System.loadLibrary("nmp_app_podcast")` in `KernelBridge.kt`'s static
   initializer maps to the `.so` Android's `PackageManager` extracted from
   the APK.

## Current status

| Gate | Status |
|---|---|
| `apps/nmp-app-podcast/Cargo.toml` adds `cdylib` | ✅ |
| `apps/nmp-app-podcast/src/android.rs` JNI shim compiles for `aarch64-linux-android` and `x86_64-linux-android` | ✅ |
| `libnmp_app_podcast.so` exports 11 `Java_io_f7z_podcast_KernelBridge_*` symbols | ✅ |
| Kotlin Compose source compiles | ⚠️ depends on local Android Studio / Gradle wrapper init |
| App boots and renders subscribed library | ⏸ stub snapshot — wired once `LibraryProjection` is serialized in M2.A+ |
| One capability hop succeeds end-to-end | ⏸ same blocker |

The Kotlin source is structurally complete; the gradle wrapper itself is
not vendored (we point at the system `gradle` binary from Android Studio).
A first-time contributor should run `gradle wrapper --gradle-version 8.7`
from `android/Podcast/` before the `gradlew` flow above works.

## Doctrine reminders

This project shares the iOS doctrine — see `AGENTS.md` at the repo root.
Relevant for the JNI layer:

- **D0** — Kernel emits; this Kotlin layer composes. The Compose UI MUST
  NOT contain podcast-domain logic; it decodes JSON and renders.
- **D5/D8** — `KernelBridge.kt` carries no cached state beyond the opaque
  handle.
- **D6** — JNI entry points return `null` / `0` / void on failure; errors
  never cross FFI.

## Open follow-ups

Tracked in `docs/BACKLOG.md` under "NMP Migration — M2.F Android proof
follow-ups": gradle wrapper vendoring, `LibraryProjection` snapshot
serializer, JNI shim relocation, real nsec entry sheet.
