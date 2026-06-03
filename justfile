PABLO_IPHONE := "3C438D9B-2021-5A30-93DB-910F7754F9A2"

# Deploy to Pablo's iPhone: rebuild Rust (static only), build Swift, install, launch
pablo-iphone-deploy:
    #!/usr/bin/env bash
    set -euo pipefail
    DEVICE="{{PABLO_IPHONE}}"

    echo "==> Rebuilding Rust for aarch64-apple-ios..."
    cargo build --target aarch64-apple-ios -p nmp-app-podcast

    # Cargo always emits both .a and .dylib. Xcode prefers .dylib and embeds
    # Mac-absolute paths in it, causing launch crashes on device. Delete the
    # dylib AFTER cargo so the Xcode linker falls back to the static .a.
    echo "==> Removing dylibs so linker uses static .a..."
    rm -f target/aarch64-apple-ios/debug/libnmp_app_podcast.dylib \
          target/aarch64-apple-ios/debug/deps/libnmp_app_podcast.dylib

    echo "==> Building Xcode (device)..."
    xcodebuild build \
        -workspace Podcastr.xcworkspace \
        -scheme Podcastr \
        -configuration Debug \
        -destination "id=$DEVICE" \
        -skipPackagePluginValidation \
        2>&1 | grep -E "error:|BUILD SUCCEEDED|BUILD FAILED|✅|❌" || true

    APP=$(xcodebuild -workspace Podcastr.xcworkspace -scheme Podcastr \
        -configuration Debug -showBuildSettings 2>/dev/null \
        | grep '^ *BUILT_PRODUCTS_DIR' | awk '{print $3}')/Podcastr.app

    echo "==> Installing on device $DEVICE..."
    xcrun devicectl device install app --device "$DEVICE" "$APP"

    echo "==> Launching..."
    xcrun devicectl device process launch --device "$DEVICE" io.f7z.podcast

    echo "✅ Done — app running on Pablo's iPhone"
