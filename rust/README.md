# Pod0 shared Rust kernel

This workspace is the additive shared-product kernel. It owns no user data yet;
the Swift application remains authoritative until a complete vertical-slice
import and cutover. The permanent operating rule is:

> Native executes platform primitives; Rust owns durable product decisions.

## Crate boundaries

- `pod0-domain` owns stable, platform-neutral value types and invariants.
- `pod0-application` owns deterministic commands, policy, projections, and
  capability contracts. Time and every other nondeterministic input enter
  through an explicit interface.
- `pod0-nmp` is the only crate allowed to depend directly on generic NMP. It
  adapts NMP's public Rust facade; Pod0 nouns never enter NMP crates.
- `pod0-facade` is the one app-owned native/core boundary. Its typed
  command/projection/event/host-request contract is documented in
  [`FACADE_CONTRACT.md`](FACADE_CONTRACT.md), and #76 will derive Swift and
  Kotlin bindings from that same source.

No crate may depend on NMP mechanism crates such as `nmp-engine`, `nmp-store`,
or `nmp-ffi`. Pod0 will not import NMP's generated Swift/Kotlin bindings as a
second bridge; the app-owned facade composes NMP inside Rust.

The bootstrap probe is deliberately non-durable. It proves crate direction,
injected time, deterministic output, and facade shape without inventing the
listening model ahead of issue #78.

## Reproducible checks

From the repository root:

```sh
./scripts/check_rust.sh
```

The script uses the exact toolchain in `rust-toolchain.toml`, the committed
lockfile, formatting and Clippy gates, workspace tests, the dependency-boundary
checker, `cargo-deny` license/source/advisory policy, and `cargo-audit`.

## Security hold on NMP consumption

The pinned NMP graph currently resolves Hickory 0.25.2, affected by
RUSTSEC-2026-0118 and RUSTSEC-2026-0119. Issue
[#85](https://github.com/pablof7z/pod0/issues/85) blocks every dependency on
`pod0-nmp` until an upstream-supported graph removes both advisories. The
workspace accepts them only for the isolated compile-and-lifecycle bootstrap;
the dependency-policy check prevents the facade or any other crate from
consuming that adapter while either exception exists. No NMP network path is
linked into the iOS app.

## NMP pin and upgrade policy

The only NMP dependency is the supported `nmp` crate at Git revision
`f3495f09c8a3f90f3b31a28313f572c09fbdb369`, audited on 2026-07-18 against
upstream `master`. NMP is pre-1.0, so an upgrade requires a named Pod0 issue,
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
