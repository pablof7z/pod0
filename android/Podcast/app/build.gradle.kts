import org.gradle.internal.os.OperatingSystem

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "io.f7z.podcast"
    compileSdk = 34

    defaultConfig {
        applicationId = "io.f7z.podcast"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }
    composeOptions { kotlinCompilerExtensionVersion = "1.5.14" }

    // `.so` files are produced by cargo-ndk into `src/main/jniLibs/<abi>` —
    // the same layout NMP's Chirp Android uses.
    sourceSets["main"].jniLibs.srcDirs("src/main/jniLibs")
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.06.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    // Full Material (M2) artifact for the classic `pullRefresh` modifier +
    // `PullRefreshIndicator`. material3 1.2.x (compose-bom 2024.06.00) predates
    // `PullToRefreshBox`, so the library pull-to-refresh uses the M2 API. Only
    // the pull-refresh surface is consumed; the app remains Material3.
    implementation("androidx.compose.material:material")
    implementation("androidx.activity:activity-compose:1.9.0")
    // `androidx.core.text.HtmlCompat` for stripping HTML from RSS show notes
    // in the episode-detail surface. Declared explicitly rather than relied on
    // transitively via activity-compose.
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.2")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.3")
    implementation("net.java.dev.jna:jna:5.14.0@aar")

    // ─── Jetpack Security — EncryptedSharedPreferences for the Nostr nsec ──
    //
    // `security/KeystoreManager.kt` persists the user's local private key as
    // an AES-256-GCM ciphertext under a hardware-backed Android Keystore key,
    // so the nsec survives restart without ever hitting disk in plaintext.
    // The 1.1.0-alpha line is the only one published for AGP 8 / minSdk 26.
    implementation("androidx.security:security-crypto:1.1.0-alpha06")

    // ─── Coil — async artwork loading from kernel-projected URLs ──────────
    //
    // Search results, library tiles, and episode detail render remote
    // artwork via `coil.compose.AsyncImage`. Coil 2.6.0 is the last 2.x
    // line; pinned (not 3.x) to stay on the kotlinx-coroutines baseline the
    // rest of the module compiles against.
    implementation("io.coil-kt:coil-compose:2.6.0")

    // ─── media3 — ExoPlayer + MediaSession for the real audio capability ──
    //
    // The capability executor (`capabilities/ExoPlayerCapability.kt`) drives
    // an `ExoPlayer` instance owned by `service/PodcastPlaybackService` so
    // playback continues in the foreground service while the activity is
    // backgrounded. The session module wires the lock-screen / Bluetooth /
    // Android Auto surfaces. The UI module is pulled in for future
    // `PlayerNotificationManager` interop.
    implementation("androidx.media3:media3-exoplayer:1.4.1")
    implementation("androidx.media3:media3-ui:1.4.1")
    implementation("androidx.media3:media3-session:1.4.1")

    // ─── OkHttp — HTTP engine for the download capability ─────────────
    //
    // `capabilities/DownloadCapability.kt` fetches episode enclosures over
    // OkHttp (streaming body → `.part` file → atomic rename). 4.12.0 is the
    // last 4.x line on the kotlin-stdlib baseline the module compiles
    // against; pinned off 5.x to stay on that baseline (matching the
    // coil/media3 pinning rationale above).
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // ─── ZXing Core — QR code generation for NIP-46 nostrconnect:// ──────
    //
    // `ui/NostrConnectScreen.kt` renders a `nostrconnect://` URI as a QR code
    // the user scans in a signer app (Amber, nsec.app, etc.). We use the core
    // encoder only — no Android-specific ZXing UI dep (those pull in old View
    // stack we don't use). The output is an `android.graphics.Bitmap` painted
    // from the QrCodeWriter pixel matrix, matching iOS CIFilter.qrCodeGenerator.
    //
    // 3.5.3 is the last stable 3.x release; matches the baseline available on
    // all Android targets in this module (minSdk 26).
    implementation("com.google.zxing:core:3.5.3")

    // ─── JUnit — local JVM unit tests ────────────────────────────────
    //
    // Runs on the host JVM (no emulator/NDK). `SnapshotCodecTest` proves the
    // push-frame envelope decode (issue #320) — that `nextUpdate()`'s
    // `{"t":"snapshot","v":{...}}` frame unwraps to the same `PodcastSnapshot`
    // the bare projection pull yields, with `rev` propagating across frames.
    testImplementation("junit:junit:4.13.2")

    // ─── Mockito — test doubles for JVM unit tests ────────────────────────────
    //
    // Used by `MediaSessionTransportRoutingTest` to create a no-op stub for the
    // media3 `Player` interface (many abstract methods; full manual
    // implementation would be brittle). The version is pinned to the 5.x line
    // (compatible with JVM 11 / Android Gradle Plugin baseline) and uses
    // kotlin-reflection-free byte-buddy mocking so no Robolectric or NDK is
    // required.
    testImplementation("org.mockito:mockito-core:5.11.0")
}

// ── cargo-ndk task ───────────────────────────────────────────────────────────
//
// Cross-compile the SAME `nmp-app-podcast` crate the iOS app links, packed as
// `libnmp_app_podcast.so` for the two shipped Android ABIs. Output lands
// directly in `jniLibs/<abi>/` for both targets.
//
// Kotlin talks to the generated UniFFI binding under
// `app/src/main/java/uniffi/nmp_app_podcast/`. The shared library still ships
// the same Rust app kernel; handwritten Android JNI entry points are not part
// of the app boundary.
//
// Manual invocation (matches what `preBuild` runs):
//
//   cd ../..  # repo root
//   ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/<version> \
//     cargo ndk -t arm64-v8a -t x86_64 \
//       --manifest-path apps/nmp-app-podcast/Cargo.toml \
//       -o android/Podcast/app/src/main/jniLibs \
//       build --release
val cargoNdk by tasks.registering(Exec::class) {
    // Run cargo from the workspace root (4 levels up: app → Podcast → android → repo).
    workingDir = rootProject.projectDir.parentFile.parentFile
    val cargo = "${System.getProperty("user.home")}/.cargo/bin/cargo"
    val bin = if (OperatingSystem.current().isWindows) "$cargo.exe" else cargo
    commandLine(
        bin, "ndk",
        "--manifest-path", "apps/nmp-app-podcast/Cargo.toml",
        "-t", "arm64-v8a", "-t", "x86_64",
        "-o", "android/Podcast/app/src/main/jniLibs",
        "build", "--release",
    )
}

tasks.named("preBuild") { dependsOn(cargoNdk) }
