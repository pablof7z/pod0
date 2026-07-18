#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
OUTPUT_ROOT=${POD0_BINDINGS_OUTPUT_ROOT:-"$REPO_ROOT/Generated/Pod0Core"}
TEMP_ROOT=$(mktemp -d /tmp/pod0-bindings.XXXXXX)

cleanup() {
  case "$TEMP_ROOT" in
    /tmp/pod0-bindings.*) rm -rf "$TEMP_ROOT" ;;
  esac
}
trap cleanup EXIT

cd "$REPO_ROOT/rust"
cargo rustc -p pod0-facade --release --locked --crate-type cdylib
CARGO_OUTPUT=$(cargo metadata --format-version 1 --no-deps --locked \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
LIBRARY="$CARGO_OUTPUT/release/libpod0_facade.dylib"

mkdir -p "$TEMP_ROOT/Swift" "$TEMP_ROOT/Kotlin" "$OUTPUT_ROOT"
cargo run -p pod0-uniffi-bindgen --locked -- generate \
  --library "$LIBRARY" \
  --config "$REPO_ROOT/rust/uniffi.toml" \
  --language swift \
  --no-format \
  --out-dir "$TEMP_ROOT/Swift"
cargo run -p pod0-uniffi-bindgen --locked -- generate \
  --library "$LIBRARY" \
  --config "$REPO_ROOT/rust/uniffi.toml" \
  --language kotlin \
  --no-format \
  --out-dir "$TEMP_ROOT/Kotlin"

while IFS= read -r -d '' generated_file; do
  perl -0777 -pi -e 's/[ \t]+$//mg; s/\s+\z/\n/' "$generated_file"
done < <(find "$TEMP_ROOT/Swift" "$TEMP_ROOT/Kotlin" -type f -print0)

rsync -a --delete "$TEMP_ROOT/Swift/" "$OUTPUT_ROOT/Swift/"
rsync -a --delete "$TEMP_ROOT/Kotlin/" "$OUTPUT_ROOT/Kotlin/"
echo "Generated Swift and Kotlin bindings in $OUTPUT_ROOT"
