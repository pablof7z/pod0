import ProjectDescription

// MARK: - Configure these before running `tuist generate`

let appName = "Podcastr"
let appDisplayName = "Podcastr"
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
        .local(path: "../ios-shake-feedback"),
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
            ],
            entitlements: .file(path: "App/Resources/Podcastr.entitlements"),
            dependencies: [
                .package(product: "P256K"),
                .package(product: "SQLiteVec"),
                .package(product: "Kingfisher"),
                .package(product: "ShakeFeedbackKit"),
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
    ],
    schemes: [
        .scheme(
            name: appName,
            buildAction: .buildAction(targets: [.target(appName), .target("\(appName)Widget")]),
            testAction: .targets([.testableTarget(target: .target("\(appName)Tests"))]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release),
            profileAction: .profileAction(configuration: .release),
            analyzeAction: .analyzeAction(configuration: .debug)
        )
    ]
)
