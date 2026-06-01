# NMP v0.2.1 Upgrade Changelog

**Old revision/tag:** `ae7b00481056a66894bec55c0817eeb9fb7b17a9` (`nmp-v0.2.0`)
**New revision/tag:** `7be4a771b59228f4b51e2ba7cfce481734d4da9a` (`nmp-v0.2.1`, workspace version `0.2.1`)
**Upstream PR:** NMP #900 — `refactor(relays): rename relay_edit_rows → configured_relays; app owns relay defaults`.
**Nature of change:** Dependency version bump. **C-ABI is byte-for-byte identical to v0.2.0 — no symbol migration required.** One small podcast-app code change was required to preserve runtime behavior (relay seeding — see below).

---

## TLDR

Pin bump of the four git-pinned NMP workspace dependencies (`nmp-app-template`,
`nmp-core`, `nmp-ffi`, `nmp-signer-broker`) from `0.2.0` to `0.2.1`. The
headline upstream change is a relay-ownership refactor: NMP renamed
`relay_edit_rows` → `configured_relays` / `RelayEditRow` → `AppRelay`, moved the
default app-relay set out of `nmp-core` into the `NmpAppBuilder` composition
root, and added JSON sidecar persistence for app relays.

The rename does not touch the podcast app (it never referenced those symbols).
The default-relay relocation, however, **does** affect the podcast app at
runtime: `nmp-core` no longer ships a hardcoded onboarding relay default, so the
podcast app now has to declare its relays explicitly. We do that with one line in
`apps/nmp-app-podcast/src/ffi/register.rs`.

### Verification

| Layer | Command | Result |
|---|---|---|
| 1 — workspace compile | `cargo check --workspace` | Pass (~41s). All NMP crates resolve at `0.2.1` / rev `7be4a771`. `Cargo.lock` updated; zero remaining references to the old rev `ae7b004`. Two pre-existing local dead-code warnings (`CompileOutcome` in `ai_chapters.rs`, unread `nostr_results` field) unrelated to NMP. |
| 2 — iOS-sim build | `cargo build -p nmp-app-podcast --target aarch64-apple-ios-sim` | Pass (~40s). No linker errors; same two pre-existing warnings only. |

---

## What changed upstream in v0.2.1 (PR #900)

1. **Rename `relay_edit_rows` → `configured_relays`, `RelayEditRow` → `AppRelay`**
   everywhere in NMP. Generated codegen Swift field `relayEditRows` →
   `configuredRelays`, type `[RelayEditRow]` → `[AppRelay]`. This is internal to
   `nmp-core` and the Chirp shell; **the podcast app referenced none of these
   symbols** (zero hits across Rust, Swift, and generated code), so nothing in
   the podcast tree had to be renamed.

2. **App owns its relay defaults.** `nmp-core` no longer carries a hardcoded
   onboarding relay default — the v0.2.0→v0.2.1 `nmp-core` diff deletes the seed
   and its assertions ("empty onboarding relays + unseeded kernel ⇒ no relays").
   A new `DEFAULT_APP_RELAYS` const lives in `nmp-app-template`'s builder
   (`relay.primal.net` `both,indexer` + `purplepag.es` `indexer`).

3. **`NmpAppBuilder` relay API.** The builder gains
   `.with_relay(url, role)` / `.with_relays(iter)`. `NmpAppBuilder::start()`
   resolves the declared relays (or `DEFAULT_APP_RELAYS`) and stages them into
   `ActorCommand::Start { initial_relays }` via
   `NmpApp::set_initial_relays_for_start`.

4. **`relay_config.rs` sidecar persistence** (`.nmp-relay-config.json`). For
   storage-backed builder apps, `start()` loads the sidecar if present, else
   persists the declared defaults on first run. This module is **private to
   `nmp-app-template`** (no `pub fn`), so downstream apps cannot reuse it
   directly.

5. **C-ABI unchanged.** No `nmp_app_*` symbol added, removed, or re-signed; the
   podcast app's `NmpCore.h` and Swift bridge are untouched.

---

## What the podcast app adopted

### Relay seeding in `register.rs` (required — not optional)

The podcast app is **not** constructed through `NmpAppBuilder`. The iOS shell
builds it over the raw C-ABI:

```
nmp_app_new() → nmp_app_podcast_register(app) → nmp_app_start(app, …)
```

Because it never runs through the builder, it never picks up `DEFAULT_APP_RELAYS`
and never benefits from `with_relay`. With the v0.2.1 removal of the `nmp-core`
hardcoded default, a pure pin bump would leave a **fresh install with zero
configured relays** — Nostr discovery (kind:10154) and publishing (kind:54 /
kind:1 / kind:1111) would silently no-op. `cargo check` and the iOS build both
stay green in that state, so the regression is invisible to compile-time gates.

Fix: seed the podcast app's relays via the non-builder seam,
`NmpApp::set_initial_relays_for_start`, immediately after the app reference is
taken in `register.rs` and before the shell calls `nmp_app_start`:

```rust
app_ref.set_initial_relays_for_start(vec![
    ("wss://relay.primal.net".to_string(), "both,indexer".to_string()),
    ("wss://purplepag.es".to_string(), "indexer".to_string()),
]);
```

These two relays/roles are exactly the template's `DEFAULT_APP_RELAYS`; the
podcast app now declares them explicitly, matching the v0.2.1 "app owns its
relays" contract. `set_initial_relays_for_start` takes `&self` and stages the
rows into `ActorCommand::Start { initial_relays }`, read once by the actor before
the first tick.

**The seed is unconditional.** That is correct *today* because the podcast app
has no relay-edit UI and persists no user relay choices (zero `configuredRelays`
references in the iOS tree), so there is nothing to clobber.

---

## What's optional next (not in this PR)

- **App-relay configuration UI.** A parallel PR is adding a relay-edit surface
  for the podcast app. When that lands, the unconditional seed above **must
  become seed-if-empty** so it never overwrites persisted user edits. This is
  flagged with a load-bearing comment at the seed site in `register.rs`.

- **Sidecar persistence for podcast relays.** NMP's `relay_config.rs`
  (`.nmp-relay-config.json`) is the builder-app persistence mechanism, but it is
  crate-private to `nmp-app-template`. If the podcast app later wants persisted,
  user-editable relays, it needs its own persistence path (or an upstream change
  exposing the sidecar API). Not required while the seed is the only source of
  relays.

- **`configured_relays` projection decode.** The renamed projection key is
  available in the snapshot but the podcast app does not decode a relay list
  today. Pairs naturally with the relay-edit UI above.
