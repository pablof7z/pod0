// swift-tools-version: 5.9
// SwiftPM package for NMP migration CLI tools.
// Tools require SwiftSyntax for AST-level editing (not regex/string replacement).

import PackageDescription

let package = Package(
    name: "MigrationTools",
    platforms: [
        .macOS(.v13)
    ],
    dependencies: [
        .package(
            url: "https://github.com/apple/swift-syntax.git",
            from: "510.0.0"
        ),
    ],
    targets: [
        // ── apply-token-swap ──────────────────────────────────────────────
        .executableTarget(
            name: "apply-token-swap",
            dependencies: [
                .product(name: "SwiftSyntax", package: "swift-syntax"),
                .product(name: "SwiftParser", package: "swift-syntax"),
                .product(name: "SwiftSyntaxBuilder", package: "swift-syntax"),
            ],
            path: "Sources/apply-token-swap"
        ),
        // ── split-features ────────────────────────────────────────────────
        .executableTarget(
            name: "split-features",
            dependencies: [
                .product(name: "SwiftSyntax", package: "swift-syntax"),
                .product(name: "SwiftParser", package: "swift-syntax"),
                .product(name: "SwiftSyntaxBuilder", package: "swift-syntax"),
            ],
            path: "Sources/split-features"
        ),
        // ── Unit tests ────────────────────────────────────────────────────
        .testTarget(
            name: "MigrationToolsTests",
            dependencies: [
                .product(name: "SwiftSyntax", package: "swift-syntax"),
                .product(name: "SwiftParser", package: "swift-syntax"),
            ],
            path: "Tests/MigrationToolsTests"
        ),
    ]
)
