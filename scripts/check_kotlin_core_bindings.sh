#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
TOOLS_ROOT="$REPO_ROOT/.build/pod0-tools"
KOTLIN_VERSION=2.4.10
KOTLIN_SHA256=473dd66c7a3ef4b182065b3da670466c1bf2773a9dbb0ed8b33a39fe9d4f876d
JRE_VERSION=21.0.11_10
JRE_SHA256=4b7a8cd23102c251c8b8be42a9a5f1263fb337cf1037f6f64b25f3070efe4b76
JNA_VERSION=5.19.1
JNA_SHA256=4fb141dd8ef6b0585ffceea4bc49602fbc6312fa977e2c488794ea3e6aafecae

mkdir -p "$TOOLS_ROOT/downloads" "$TOOLS_ROOT/smoke"
KOTLIN_ARCHIVE="$TOOLS_ROOT/downloads/kotlin-compiler-$KOTLIN_VERSION.zip"
JRE_ARCHIVE="$TOOLS_ROOT/downloads/temurin-jre-$JRE_VERSION.tar.gz"
JNA_JAR="$TOOLS_ROOT/downloads/jna-$JNA_VERSION.jar"
KOTLIN_HOME="$TOOLS_ROOT/kotlin-$KOTLIN_VERSION/kotlinc"
JAVA_HOME="$TOOLS_ROOT/temurin-jre-$JRE_VERSION/Contents/Home"
export JAVA_HOME

if [[ ! -f "$KOTLIN_ARCHIVE" ]]; then
  curl -fL "https://github.com/JetBrains/kotlin/releases/download/v$KOTLIN_VERSION/kotlin-compiler-$KOTLIN_VERSION.zip" \
    -o "$KOTLIN_ARCHIVE"
fi
echo "$KOTLIN_SHA256  $KOTLIN_ARCHIVE" | shasum -a 256 -c -
if [[ ! -x "$KOTLIN_HOME/bin/kotlinc" ]]; then
  mkdir -p "$TOOLS_ROOT/kotlin-$KOTLIN_VERSION"
  unzip -q "$KOTLIN_ARCHIVE" -d "$TOOLS_ROOT/kotlin-$KOTLIN_VERSION"
fi

if [[ ! -f "$JRE_ARCHIVE" ]]; then
  curl -fL "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.11%2B10/OpenJDK21U-jre_aarch64_mac_hotspot_21.0.11_10.tar.gz" \
    -o "$JRE_ARCHIVE"
fi
echo "$JRE_SHA256  $JRE_ARCHIVE" | shasum -a 256 -c -
if [[ ! -x "$JAVA_HOME/bin/java" ]]; then
  mkdir -p "$TOOLS_ROOT/temurin-jre-$JRE_VERSION"
  tar -xzf "$JRE_ARCHIVE" -C "$TOOLS_ROOT/temurin-jre-$JRE_VERSION" --strip-components=1
fi

if [[ ! -f "$JNA_JAR" ]]; then
  curl -fL "https://repo1.maven.org/maven2/net/java/dev/jna/jna/$JNA_VERSION/jna-$JNA_VERSION.jar" \
    -o "$JNA_JAR"
fi
echo "$JNA_SHA256  $JNA_JAR" | shasum -a 256 -c -

cd "$REPO_ROOT/rust"
cargo rustc -p pod0-facade --release --locked --crate-type cdylib
CARGO_OUTPUT=$(cargo metadata --format-version 1 --no-deps --locked \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')

KOTLIN_SOURCES=()
while IFS= read -r source; do
  KOTLIN_SOURCES+=("$source")
done < <(find "$REPO_ROOT/Generated/Pod0Core/Kotlin" -name '*.kt' -type f | sort)
SMOKE_SOURCES=()
while IFS= read -r source; do
  SMOKE_SOURCES+=("$source")
done < <(find "$REPO_ROOT/BindingsSmoke/Kotlin" -name '*.kt' -type f | sort)
"$KOTLIN_HOME/bin/kotlinc" \
  "${KOTLIN_SOURCES[@]}" \
  "${SMOKE_SOURCES[@]}" \
  -classpath "$JNA_JAR" \
  -jvm-target 21 \
  -include-runtime \
  -d "$TOOLS_ROOT/smoke/pod0-core-bindings.jar"
"$JAVA_HOME/bin/java" \
  -Djna.library.path="$CARGO_OUTPUT/release" \
  -classpath "$TOOLS_ROOT/smoke/pod0-core-bindings.jar:$JNA_JAR" \
  MainKt \
  "$REPO_ROOT/Fixtures/CoreSchema/schema-status-v1.properties" \
  "$REPO_ROOT/Fixtures/CoreListening/listening-domain-v1.properties" \
  "$REPO_ROOT/Fixtures/CoreImport/legacy-listening-v1.json" \
  "$REPO_ROOT/Fixtures/CoreKnowledge/recall-projection-v1.properties" \
  "$REPO_ROOT/Fixtures/CoreKnowledge/note-projection-v1.properties" \
  "$REPO_ROOT/Fixtures/CoreKnowledge/clip-projection-v1.properties" \
  "$REPO_ROOT/Fixtures/CoreKnowledge/transcript-contract-v1.properties" \
  "$REPO_ROOT/Fixtures/CoreKnowledge/chapter-contract-v1.properties"
echo "Kotlin generated binding compile and runtime smoke passed"
