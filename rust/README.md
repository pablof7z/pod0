# Pod0 shared Rust kernel

This workspace is Pod0's shared-product kernel. Migrated listening, playback,
evidence, notes, and clips use its durable stores; other domains remain native
or staged until their complete vertical-slice cutover. The permanent operating
rule is:

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
- `pod0-nmp` is the only crate allowed to depend directly on generic NMP. It
  adapts NMP's public Rust facade; Pod0 nouns never enter NMP crates.
- `pod0-facade` is the one app-owned native/core boundary. Its typed
  command/projection/event/host-request contract is documented in
  [`FACADE_CONTRACT.md`](FACADE_CONTRACT.md). Swift and Kotlin bindings derive
  from that same source and are committed under `Generated/Pod0Core`.

No crate may depend on NMP mechanism crates such as `nmp-engine`, `nmp-store`,
or `nmp-ffi`. Pod0 will not import NMP's generated Swift/Kotlin bindings as a
second bridge; the app-owned facade composes NMP inside Rust.

The app-owned facade is the typed single-writer boundary used by the migrated
listening, playback, evidence, note, clip, and recall slices. Its dispatch path
remains fire-and-forget; durable work reports through bounded state projections
and typed host requests. Full-transcript contract and storage support are
additive until the transcript importer and native adapter complete their staged
single-writer cutover.

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

## NMP security posture

The pinned NMP graph resolves Hickory 0.26.1 and contains neither
RUSTSEC-2026-0118 nor RUSTSEC-2026-0119. `cargo deny` and `cargo audit` run
without advisory exceptions. The `pod0-nmp` adapter is therefore eligible for
composition behind the app-owned Rust facade; no NMP network path is linked
into the iOS app until a product slice explicitly adds that dependency and its
lifecycle tests.

## NMP pin and upgrade policy

The only NMP dependency is the supported `nmp` crate at Git revision
`68310f88a31bf80e6b73d018b1374e73efda0041`, merged to and audited against
upstream `master` on 2026-07-19. NMP is pre-1.0, so an upgrade requires a named Pod0 issue,
review of upstream `README.md`, `docs/known-gaps.md`,
`docs/architecture/supported-surface.md`, and the current public facade, then:

1. update the exact `rev` and lockfile;
2. update the dependency-policy checker in the same commit;
3. run the Pod0 adapter lifecycle test and full workspace checks;
4. run upstream's `cargo test -p nmp-consumer-check` at the selected revision;
5. record any Swift/Kotlin/Android surface gaps that affect Pod0.

At the pinned revision, NMP's Swift wrapper is host-tested and its simulator
slices compile, while its Kotlin wrapper is a desktop-JVM falsifier rather than
an Android AAR. Pod0 therefore consumes direct Rust now and treats Android
target/binding compilation as readiness evidence only; it does not authorize
Android product work before the M5 gate.
