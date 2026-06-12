import ProjectDescription

// MARK: - Configure these before running `tuist generate`

let appName = "Podcastr"
let appDisplayName = "Pod0"
let appleTeamID = "456SHKPP26"
let deploymentTarget: DeploymentTargets = .iOS("26.0")

// MARK: - Derived identifiers

// `appBundleID` is fixed (not derived from `appName`) so renaming the working
// title doesn't invalidate the existing provisioning profile / TestFlight /
// App Store record tied to `io.f7z.podcast`.
let appBundleID = "io.f7z.podcast"
// App Group identifier is hardcoded (does not follow the bundle-ID derivation
// pattern) so the working title can change without re-provisioning the group.
let appGroupID = "group.com.podcastr.app"
let widgetBundleID = "\(appBundleID).widget"

// MARK: - Project

let project = Project(
    name: appName,
    organizationName: "f7z",
    options: .options(
        automaticSchemesOptions: .disabled,
        developmentRegion: "en"
    ),
    packages: [
        .remote(
            url: "https://github.com/GigaBitcoin/secp256k1.swift",
            requirement: .upToNextMajor(from: "0.23.1")
        ),
        // Lane 6 — RAG: on-device vector store via sqlite-vec.
        // Hosts both the `vec0` virtual table for embeddings and `fts5` for
        // hybrid lexical search in a single SQLite file.
        .remote(
            url: "https://github.com/jkrukowski/SQLiteVec",
            requirement: .upToNextMinor(from: "0.0.14")
        ),
        // Kingfisher — memory + disk image cache. Backs `CachedAsyncImage`
        // so artwork URLs (subscription / episode covers, iTunes Search
        // results, etc.) fetch at most once per session instead of
        // re-downloading every appearance like SwiftUI's stock `AsyncImage`.
        .remote(
            url: "https://github.com/onevcat/Kingfisher",
            requirement: .upToNextMajor(from: "8.0.0")
        ),
        // Pinned to a revision (not a version range) because LiteRTLM declares
        // `unsafeFlags(["-Xlinker", "-all_load"])`, which SwiftPM forbids on a
        // versioned remote dependency. A revision pin is allowed to carry unsafe
        // flags. Revision = the 0.13.0 release commit.
        .remote(
            url: "https://github.com/google-ai-edge/LiteRT-LM",
            requirement: .revision("bbc5181df03c6962d7786ce4ad72c8565232d2b2")
        ),
    ],
    settings: .settings(
        base: [
            "SWIFT_VERSION": "6.0",
            "SWIFT_STRICT_CONCURRENCY": "complete",
            "DEVELOPMENT_TEAM": "\(appleTeamID)",
            "CODE_SIGN_STYLE": "Automatic",
            "ENABLE_USER_SCRIPT_SANDBOXING": "YES",
        ]
    ),
    targets: [
        .target(
            name: appName,
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: appBundleID,
            deploymentTargets: deploymentTarget,
            infoPlist: .file(path: "App/Resources/Info.plist"),
            sources: ["App/Sources/**"],
            resources: [
                "App/Resources/Assets.xcassets",
                "App/Resources/whats-new.json",
                "App/Resources/test-episode.mp3",
                // BERT uncased WordPiece vocab for the on-device MiniLM embedder
                // (issue #236). The 384-dim Core ML model itself is a post-install
                // download (not bundled); the vocab is small (~230 KB) so it ships
                // in the IPA to keep the tokenizer dependency-free.
                "App/Resources/bert-vocab.txt",
            ],
            entitlements: .file(path: "App/Resources/Podcastr.entitlements"),
            scripts: [
                // The Rust kernel links dynamically (`-lnmp_app_podcast` resolves
                // the `.dylib` over the `.a`), which keeps the app binary from
                // absorbing a second Rust archive under LiteRTLM's `-all_load`.
                // Cargo stamps the dylib's install name as an absolute Mac path,
                // which does not exist on a device → `dyld` launch crash
                // ("Library not loaded: /Users/.../libnmp_app_podcast.dylib").
                //
                // This MUST run as a `.pre` (before the link step): the linker
                // records the dylib's install name as the dependency load command
                // in the app binary, so the id has to read `@rpath/...` *before*
                // linking. Doing it post-link (the embed step below) is too late —
                // the recorded load command would stay absolute and still crash.
                .pre(
                    script: """
                    #!/bin/bash
                    set -e
                    if [[ "$PLATFORM_NAME" == "iphonesimulator" ]]; then
                        RUST_TARGET="aarch64-apple-ios-sim"
                    else
                        RUST_TARGET="aarch64-apple-ios"
                    fi
                    DYLIB="${SRCROOT}/target/${RUST_TARGET}/debug/libnmp_app_podcast.dylib"
                    if [ ! -f "$DYLIB" ]; then
                        echo "warning: ${DYLIB} not found — skipping Rust dylib install-name fix"
                        exit 0
                    fi
                    install_name_tool -id "@rpath/libnmp_app_podcast.dylib" "$DYLIB"
                    """,
                    name: "Fix Rust Dylib Install Name",
                    basedOnDependencyAnalysis: false
                ),
                // Embed the (now @rpath) Rust dylib into the app bundle so the
                // loader finds it at `@rpath/libnmp_app_podcast.dylib` (the app's
                // `@executable_path/Frameworks` rpath). Running as a build phase
                // lets Xcode's subsequent signing step cover the dylib with the
                // real development certificate — iOS rejects ad-hoc signing — and
                // we also sign here so it is valid before the bundle is sealed.
                .post(
                    script: """
                    #!/bin/bash
                    set -e
                    if [[ "$PLATFORM_NAME" == "iphonesimulator" ]]; then
                        RUST_TARGET="aarch64-apple-ios-sim"
                    else
                        RUST_TARGET="aarch64-apple-ios"
                    fi
                    DYLIB="${SRCROOT}/target/${RUST_TARGET}/debug/libnmp_app_podcast.dylib"
                    if [ ! -f "$DYLIB" ]; then
                        echo "warning: ${DYLIB} not found — skipping Rust dylib embed"
                        exit 0
                    fi
                    DEST="${BUILT_PRODUCTS_DIR}/${FRAMEWORKS_FOLDER_PATH}/libnmp_app_podcast.dylib"
                    mkdir -p "${BUILT_PRODUCTS_DIR}/${FRAMEWORKS_FOLDER_PATH}"
                    cp -f "$DYLIB" "$DEST"
                    if [ -n "${EXPANDED_CODE_SIGN_IDENTITY:-}" ]; then
                        /usr/bin/codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY}" \
                            --timestamp=none "$DEST"
                    fi
                    """,
                    name: "Embed Rust Dylib",
                    basedOnDependencyAnalysis: false
                ),
            ],
            dependencies: [
                .package(product: "P256K"),
                .package(product: "SQLiteVec"),
                .package(product: "Kingfisher"),
                .package(product: "LiteRTLM"),
                .target(name: "\(appName)Widget"),
            ],
            settings: .settings(
                base: [
                    "APP_BUNDLE_IDENTIFIER": "\(appBundleID)",
                    "APP_GROUP_IDENTIFIER": "\(appGroupID)",
                    "PRODUCT_BUNDLE_IDENTIFIER": "$(APP_BUNDLE_IDENTIFIER)",
                    "CFBundleDisplayName": "\(appDisplayName)",
                    "GENERATE_INFOPLIST_FILE": "NO",
                    "ASSETCATALOG_COMPILER_APPICON_NAME": "AppIcon",
                    "TARGETED_DEVICE_FAMILY": "1,2",
                    "PROVISIONING_PROFILE_SPECIFIER": "$(CI_APP_PROFILE_SPECIFIER)",
                    // Rust FFI bridge
                    "SWIFT_OBJC_BRIDGING_HEADER": "App/Sources/Bridge/NmpCore.h",
                    "OTHER_LDFLAGS": "$(inherited) -lnmp_app_podcast",
                    "ENABLE_USER_SCRIPT_SANDBOXING": "NO",
                    "LIBRARY_SEARCH_PATHS[sdk=iphoneos*]": "$(inherited) $(SRCROOT)/target/aarch64-apple-ios/debug $(SRCROOT)/target/aarch64-apple-ios/release $(HOME)/.cargo/target-shared/aarch64-apple-ios/debug $(HOME)/.cargo/target-shared/aarch64-apple-ios/release",
                    "LIBRARY_SEARCH_PATHS[sdk=iphonesimulator*]": "$(inherited) $(SRCROOT)/target/aarch64-apple-ios-sim/debug $(SRCROOT)/target/aarch64-apple-ios-sim/release $(HOME)/.cargo/target-shared/aarch64-apple-ios-sim/debug $(HOME)/.cargo/target-shared/aarch64-apple-ios-sim/release",
                ]
            )
        ),
        // MARK: - Widget extension
        .target(
            name: "\(appName)Widget",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: widgetBundleID,
            deploymentTargets: deploymentTarget,
            infoPlist: .file(path: "App/Widget/Resources/Info.plist"),
            sources: ["App/Widget/Sources/**"],
            resources: [],
            entitlements: .file(path: "App/Widget/Resources/PodcastrWidget.entitlements"),
            dependencies: [],
            settings: .settings(
                base: [
                    "APP_BUNDLE_IDENTIFIER": "\(widgetBundleID)",
                    "APP_GROUP_IDENTIFIER": "\(appGroupID)",
                    "PRODUCT_BUNDLE_IDENTIFIER": "$(APP_BUNDLE_IDENTIFIER)",
                    "CFBundleDisplayName": "\(appDisplayName)",
                    "GENERATE_INFOPLIST_FILE": "NO",
                    "TARGETED_DEVICE_FAMILY": "1,2",
                    "SWIFT_VERSION": "6.0",
                    "SWIFT_STRICT_CONCURRENCY": "complete",
                    "PROVISIONING_PROFILE_SPECIFIER": "$(CI_WIDGET_PROFILE_SPECIFIER)",
                ]
            )
        ),
        .target(
            name: "\(appName)Tests",
            destinations: [.iPhone],
            product: .unitTests,
            bundleId: "\(appBundleID).tests",
            deploymentTargets: deploymentTarget,
            sources: ["AppTests/Sources/**"],
            // Cross-language parity fixtures, each emitted by a Rust test and
            // decoded by a matching Swift test:
            //   - settings_fresh_install.json        → SettingsSnapshotParityTests
            //   - podcast_update_with_widget.json    → PlatformWidgetContractTests
            //     (decoded through the bridge's .convertFromSnakeCase config —
            //     pins Rust-JSON ↔ embedded-WidgetSnapshot compatibility).
            //   - podcast_update_with_chapters.json  → PodcastUpdateChapterDecodeTests
            //     (chapters + transcript_entries embedded in PodcastUpdate —
            //     guards against NaN→null required-field frame-drop; #371-class).
            resources: [
                "tests/fixtures/settings_fresh_install.json",
                "tests/fixtures/podcast_update_with_widget.json",
                "tests/fixtures/podcast_update_with_chapters.json",
            ],
            dependencies: [.target(name: appName)],
            settings: .settings(
                base: [
                    "GENERATE_INFOPLIST_FILE": "YES",
                    "PRODUCT_BUNDLE_IDENTIFIER": "\(appBundleID).tests",
                    "BUNDLE_LOADER": "$(TEST_HOST)",
                    "TEST_HOST": "$(BUILT_PRODUCTS_DIR)/\(appName).app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/\(appName)",
                ]
            )
        ),
        // MARK: - Black-box UI test runner (device scenario tests)
        //
        // Intentionally has NO dependency on the app target: it drives the
        // already-installed `io.f7z.podcast` build via
        // `XCUIApplication(bundleIdentifier:)`. This keeps the device test
        // loop fast (only the tiny runner builds — no Rust cross-compile, no
        // app relink) and tests the exact bytes a user has on device.
        .target(
            name: "\(appName)UITests",
            destinations: [.iPhone],
            product: .uiTests,
            bundleId: "\(appBundleID).uitests",
            deploymentTargets: deploymentTarget,
            sources: ["AppUITests/Sources/**"],
            dependencies: [],
            settings: .settings(
                base: [
                    "GENERATE_INFOPLIST_FILE": "YES",
                    "PRODUCT_BUNDLE_IDENTIFIER": "\(appBundleID).uitests",
                    "DEVELOPMENT_TEAM": "\(appleTeamID)",
                    "CODE_SIGN_STYLE": "Automatic",
                    "SWIFT_VERSION": "6.0",
                    "TARGETED_DEVICE_FAMILY": "1,2",
                ]
            )
        ),
    ],
    schemes: [
        .scheme(
            name: appName,
            buildAction: .buildAction(targets: [.target(appName), .target("\(appName)Widget")]),
            testAction: .targets([
                .testableTarget(target: .target("\(appName)Tests")),
                .testableTarget(target: .target("\(appName)UITests")),
            ]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release),
            profileAction: .profileAction(configuration: .release),
            analyzeAction: .analyzeAction(configuration: .debug)
        ),
        // Dedicated UI-test scheme: builds ONLY the runner so a device test
        // run does not rebuild the app or Rust kernel.
        .scheme(
            name: "\(appName)UITests",
            buildAction: .buildAction(targets: [.target("\(appName)UITests")]),
            testAction: .targets([.testableTarget(target: .target("\(appName)UITests"))]),
            runAction: .runAction(configuration: .debug)
        ),
    ]
)
