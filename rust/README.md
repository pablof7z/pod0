# Pod0 shared Rust kernel

This workspace is Pod0's shared-product kernel. Migrated listening, playback,
transcript, evidence, note, and clip slices use its durable stores; other
domains remain native or staged until their complete vertical-slice cutover.
The permanent operating rule is:

> Native executes platform primitives; Rust owns durable product decisions.

## Crate boundaries

- `pod0-domain` owns stable, platform-neutral value types and invariants.
- `pod0-application` owns deterministic commands, policy, projections, and
  capability contracts. Time and every other nondeterministic input enter
  through an explicit interface.
- `pod0-storage` owns versioned app-core SQLite schemas, transactional
  migrations, verified backups, recovery state, and domain cutover markers.
  Its current schema is infrastructure-only and imports no Swift records. See
  [`SCHEMA_MIGRATIONS.md`](SCHEMA_MIGRATIONS.md).
- `pod0-facade` is the one app-owned native/core boundary. Its typed
  command/projection/event/host-request contract is documented in
  [`FACADE_CONTRACT.md`](FACADE_CONTRACT.md). Swift and Kotlin bindings derive
  from that same source and are committed under `Generated/Pod0Core`.
- `pod0-cli` is the headless platform shell. It opens or creates isolated
  authoritative stores, maps a versioned JSON protocol to facade commands and
  projections, and executes supported HTTP and agent-model host requests.
  Unsupported media capabilities remain explicitly unavailable.

limited to product nouns, authorization, and durable product state.

The app-owned facade is the typed single-writer boundary used by the migrated
listening, playback, transcript, evidence, note, clip, and recall slices. Its
dispatch path remains fire-and-forget; durable work reports through bounded
state projections and typed host requests. The transcript store becomes
authoritative only after its verified legacy import commits the selection,
episode readiness, listening revision, and cutover marker atomically.

## Headless CLI

Run newline-delimited JSON over stdin/stdout:

```sh
cargo run -p pod0-cli -- --json --create --store ./pod0.sqlite
```

Use `--repl` for the interactive shell. Agent turns require a configured
endpoint and model: `POD0_OPENAI_BASE_URL` plus optional
`POD0_OPENAI_API_KEY`, or `POD0_OLLAMA_BASE_URL`/`OLLAMA_HOST`, together with
`POD0_AGENT_MODEL`. Credentials and endpoint values are never returned by the
CLI protocol.

## Reproducible checks

From the repository root:

```sh
./scripts/check_rust.sh
```

The script uses the exact toolchain in `rust-toolchain.toml`, the committed
lockfile, formatting and Clippy gates, workspace tests, the dependency-boundary
checker, `cargo-deny` license/source/advisory policy, and `cargo-audit`.
It also verifies that shipped SQL migration files match their sequential
schema version and SHA-256 lock.

Regenerate or verify the language bindings with:

```sh
./scripts/generate_core_bindings.sh
./scripts/check_core_binding_drift.sh
./scripts/check_kotlin_core_bindings.sh
./scripts/check_core_portability.sh
```

`generate_core_bindings.sh` invokes the in-workspace UniFFI 0.32.0 CLI and
updates both generated languages atomically. The drift check regenerates into a
temporary directory and compares every file. The Kotlin check uses pinned,
SHA-verified Kotlin, Temurin, and JNA artifacts to compile and exercise the
generated facade. `ci_scripts/bootstrap_project.sh` builds deterministic arm64
iOS device and simulator static libraries into the ignored
`.build/pod0core/Pod0CoreFFI.xcframework` before Tuist generates the project.
The bootstrap then normalizes Tuist's local binary reference to `SOURCE_ROOT`
so the committed Xcode project contains no checkout-specific absolute path.
The portability check pins cargo-ndk 4.1.2, Android NDK 26.3.11579264, and API
23; it checks every workspace crate on Android arm64 and links facade libraries
for Android arm64/x86_64. Those results prove API portability, not permission
to begin the M6 Android application phase; the M5 product/architecture gate
remains authoritative.


The upstream Swift SDK is prepared at Git revision
`bca64d75eeee8496b93ca220976c4fa6046cf6cb` by
review of its public Swift surface, then:

1. update the exact revision in the preparation script;
3. run upstream Swift tests and Pod0's full Apple build/tests;
4. record any Swift/Kotlin/Android surface gaps that affect Pod0.
