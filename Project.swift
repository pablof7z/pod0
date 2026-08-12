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
let shareBundleID = "\(appBundleID).share"
let coreBindingsName = "Pod0Core"

// MARK: - Project

let project = Project(
    name: appName,
    organizationName: "f7z",
    options: .options(
        automaticSchemesOptions: .disabled,
        developmentRegion: "en"
    ),
    packages: [
        // Prepared from the exact Rust dependency revision by
        // `scripts/prepare_nmp_swift_package.sh`. NMP intentionally builds
        // its Swift bindings and XCFramework from the same source revision.
        .local(path: ".build/nmp/Packages/NMP"),
        // Kingfisher — memory + disk image cache. Backs `CachedAsyncImage`
        // so artwork URLs (subscription / episode covers, iTunes Search
        // results, etc.) fetch at most once per session instead of
        // re-downloading every appearance like SwiftUI's stock `AsyncImage`.
        .remote(
            url: "https://github.com/onevcat/Kingfisher",
            requirement: .exact("8.9.0")
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
            name: coreBindingsName,
            destinations: [.iPhone, .iPad],
            product: .framework,
            bundleId: "\(appBundleID).core",
            deploymentTargets: deploymentTarget,
            infoPlist: .default,
            sources: ["Generated/Pod0Core/Swift/*.swift"],
            // The xcframework is an untracked local artifact Xcode has no
            // dependency edge to, so a stale Rust core links silently against
            // regenerated bindings and only aborts at runtime. Fail the build
            // instead.
            scripts: [
                .pre(
                    script: """
                    "$SRCROOT/scripts/check_core_binding_freshness.sh"
                    """,
                    name: "Check Pod0Core binding freshness",
                    // Declared so user-script sandboxing grants read access;
                    // the check itself always runs.
                    inputPaths: [
                        "$(SRCROOT)/scripts/check_core_binding_freshness.sh",
                        "$(SRCROOT)/Generated/Pod0Core/bindings.fingerprint",
                        "$(SRCROOT)/.build/pod0core/Pod0CoreFFI.xcframework/bindings.fingerprint",
                    ],
                    basedOnDependencyAnalysis: false
                )
            ],
            dependencies: [
                .xcframework(
                    path: .relativeToRoot(".build/pod0core/Pod0CoreFFI.xcframework")
                ),
                .sdk(name: "SystemConfiguration", type: .framework),
            ],
            settings: .settings(
                base: [
                    "SWIFT_VERSION": "6.0",
                    "SWIFT_STRICT_CONCURRENCY": "complete",
                    "SKIP_INSTALL": "YES",
                    // Keep this static XCFramework's processed headers away
                    // from NMP's static XCFramework headers. Both correctly
                    // contain module.modulemap, which otherwise collide in
                    // Xcode's shared Products/include directory.
                    "CONFIGURATION_BUILD_DIR": "$(BUILD_DIR)/Pod0Core/$(CONFIGURATION)$(EFFECTIVE_PLATFORM_NAME)",
                ]
            )
        ),
        .target(
            name: appName,
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: appBundleID,
            deploymentTargets: deploymentTarget,
            infoPlist: .file(path: "App/Resources/Info.plist"),
            sources: [
                "App/Sources/**",
                "App/Shared/Sources/**",
            ],
            resources: [
                "App/Resources/Assets.xcassets",
            ],
            entitlements: .file(path: "App/Resources/Podcastr.entitlements"),
            dependencies: [
                .package(product: "NMP"),
                .package(product: "Kingfisher"),
                .target(name: coreBindingsName),
                .target(name: "\(appName)Widget"),
                .target(name: "\(appName)Share"),
            ],
            settings: .settings(
                base: [
                    "APP_BUNDLE_IDENTIFIER": "\(appBundleID)",
                    "APP_GROUP_IDENTIFIER": "\(appGroupID)",
                    "PRODUCT_BUNDLE_IDENTIFIER": "$(APP_BUNDLE_IDENTIFIER)",
                    "CFBundleDisplayName": "\(appDisplayName)",
                    "GENERATE_INFOPLIST_FILE": "NO",
                    "OTHER_LDFLAGS": "$(inherited) -lsqlite3",
                    "FRAMEWORK_SEARCH_PATHS": "$(inherited) $(BUILD_DIR)/Pod0Core/$(CONFIGURATION)$(EFFECTIVE_PLATFORM_NAME)",
                    "HEADER_SEARCH_PATHS[sdk=iphoneos*]": "$(inherited) $(SRCROOT)/.build/pod0core/Pod0CoreFFI.xcframework/ios-arm64/Headers",
                    "HEADER_SEARCH_PATHS[sdk=iphonesimulator*]": "$(inherited) $(SRCROOT)/.build/pod0core/Pod0CoreFFI.xcframework/ios-arm64_x86_64-simulator/Headers",
                    "ASSETCATALOG_COMPILER_APPICON_NAME": "AppIcon",
                    "ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME": "",
                    "TARGETED_DEVICE_FAMILY": "1,2",
                    "PROVISIONING_PROFILE_SPECIFIER": "$(CI_APP_PROFILE_SPECIFIER)",
                    "SWIFT_INCLUDE_PATHS": "$(SRCROOT)/App/Support",
                ],
                configurations: [
                    .release(
                        name: "Release",
                        settings: ["ENABLE_TESTABILITY": "YES"]
                    ),
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
        // MARK: - Share extension
        .target(
            name: "\(appName)Share",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: shareBundleID,
            deploymentTargets: deploymentTarget,
            infoPlist: .file(path: "App/ShareExtension/Resources/Info.plist"),
            sources: [
                "App/ShareExtension/Sources/**",
                "App/Shared/Sources/**",
            ],
            resources: [],
            entitlements: .file(
                path: "App/ShareExtension/Resources/PodcastrShare.entitlements"
            ),
            dependencies: [],
            settings: .settings(
                base: [
                    "APP_BUNDLE_IDENTIFIER": "\(shareBundleID)",
                    "APP_GROUP_IDENTIFIER": "\(appGroupID)",
                    "PRODUCT_BUNDLE_IDENTIFIER": "$(APP_BUNDLE_IDENTIFIER)",
                    "CFBundleDisplayName": "Add to Pod0",
                    "GENERATE_INFOPLIST_FILE": "NO",
                    "TARGETED_DEVICE_FAMILY": "1,2",
                    "SWIFT_VERSION": "6.0",
                    "SWIFT_STRICT_CONCURRENCY": "complete",
                    "PROVISIONING_PROFILE_SPECIFIER": "$(CI_SHARE_PROFILE_SPECIFIER)",
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
            resources: [
                "Fixtures/CoreSchema/**",
                "Fixtures/CoreListening/**",
                "Fixtures/CoreImport/**",
                "Fixtures/CoreKnowledge/**",
            ],
            dependencies: [
                .target(name: appName),
                .target(name: coreBindingsName),
            ],
            settings: .settings(
                base: [
                    "GENERATE_INFOPLIST_FILE": "YES",
                    "OTHER_LDFLAGS": "$(inherited) -lsqlite3",
                    "PRODUCT_BUNDLE_IDENTIFIER": "\(appBundleID).tests",
                    "FRAMEWORK_SEARCH_PATHS": "$(inherited) $(BUILD_DIR)/Pod0Core/$(CONFIGURATION)$(EFFECTIVE_PLATFORM_NAME)",
                    "HEADER_SEARCH_PATHS[sdk=iphoneos*]": "$(inherited) $(SRCROOT)/.build/pod0core/Pod0CoreFFI.xcframework/ios-arm64/Headers",
                    "HEADER_SEARCH_PATHS[sdk=iphonesimulator*]": "$(inherited) $(SRCROOT)/.build/pod0core/Pod0CoreFFI.xcframework/ios-arm64_x86_64-simulator/Headers",
                    "BUNDLE_LOADER": "$(TEST_HOST)",
                    "TEST_HOST": "$(BUILT_PRODUCTS_DIR)/\(appName).app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/\(appName)",
                    "SWIFT_INCLUDE_PATHS": "$(SRCROOT)/App/Support",
                ]
            )
        ),
    ],
    schemes: [
        .scheme(
            name: appName,
            buildAction: .buildAction(targets: [
                .target(appName),
                .target("\(appName)Widget"),
                .target("\(appName)Share"),
            ]),
            testAction: .targets([.testableTarget(target: .target("\(appName)Tests"))]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release),
            profileAction: .profileAction(configuration: .release),
            analyzeAction: .analyzeAction(configuration: .debug)
        )
    ]
)
