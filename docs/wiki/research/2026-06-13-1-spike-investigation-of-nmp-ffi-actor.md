---
type: research-record
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
source_lines: 8869-8980
agent_attribution: a098817b8d5deaf4b
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Spike investigation of NMP FFI/actor surface for retiring app-local per-podcast crypto; surveyed EXISTS vs MISSING capabilities, found signing API already present but identified two upstream gaps (hidden-account flag, non-active-key persistence); verdict PARTIALLY ACTIONABLE"
captured_at: 2026-06-13T18:48:12Z
---

Spike investigation of NMP FFI/actor surface for retiring app-local per-podcast crypto; surveyed EXISTS vs MISSING capabilities, found signing API already present but identified two upstream gaps (hidden-account flag, non-active-key persistence); verdict PARTIALLY ACTIONABLE

---

User: [Agent task result: Agent "Cycle-11 #2: blossom/NIP-F4 upstream spike" completed]
This confirms a second gap: NMP only persists the **active** local account (`enqueue_persist_current_active_session`). A per-podcast key added via `AddSigner { make_active: false }` would NOT survive a kernel restart through NMP's keyring — which is exactly why the app maintains its own persisted `PodcastKeyStore`. 

I now have a complete, evidence-backed picture. Let me compile the final report.

## SPIKE FINDINGS: Retiring the app-local Nostr-signing + Blossom re-implementation for per-podcast NIP-F4 keys

Verified against `origin/main` and the pinned NMP rev `6418a7acae78b27ecd7a5a5c102db43a05c1cf6c` (vendored at `~/.cargo/git/checkouts/nostr-multi-platform-88ad235835bdcf46/6418a7a`, the workspace pin in `Cargo.toml`).

---

### 1. What app-local crypto exists, and what it duplicates

**App-side re-implementation (the wart):**

- `apps/nmp-app-podcast/src/blossom.rs` — `upload_to_blossom()` (`:62`) hand-rolls the entire BUD-02 pipeline: `std::fs::read` the file, `sha256_hex` (`:118`), `build_auth_event()` (`:128`) which calls `SecretKey::from_slice(secret_bytes)` + `Keys::new` + `EventBuilder::new(Kind::from(24242))…sign_with_keys(&keys)`, then base64 the JSON into an `Authorization: Nostr …` header and PUT via the app's HTTP capability. Raw secp256k1 secret bytes are in app code.
- `apps/nmp-app-podcast/src/host_op_publish.rs` — `sign_event()` (`:367`) does `SecretKey::from_slice(secret_bytes)` + `Keys::new` + `EventBuilder…sign_with_keys` for kind:10154 (`publish_show`, sign at `:151`) and kind:54 (`publish_episode`, sign at `:259`). The pre-signed event is then routed via `publish_via_nmp` → `nmp.publish { Publish, target: Auto }` (`nmp_dispatch.rs:24`), i.e. the app signs and NMP only relays.
- Header comment at `host_op_publish.rs:1-10` admits the gap: *"Per-podcast events … are signed in Rust with the podcast's secp256k1 key … The remaining per-podcast signing gap (D4/D7) will be closed once NMP exposes a 'sign-as-non-active-account' API."*
- Keys live in `state.publish.podcast_keys` (`PodcastKeyStore`), accessed at `host_op_publish.rs:76, 116, 193, 326, 348`. This is the app's own persisted store, NOT the NMP roster.

**Kernel/NMP capability it duplicates:**

- `nmp-blossom/src/upload/mod.rs` — `BlossomUploadCommand::run` streams sha256 (`hash_file_streaming`), `auth::build_upload_auth` (`nmp-blossom/src/auth.rs:42`), signs via the generic `build_sign_event_for_account(unsigned, signer_pubkey, continuation)` port, builds the `Nostr <base64>` header (`auth.rs:authorization_header_value`), and PUTs to each server. This is byte-for-byte the same protocol the app reimplements — including the `expiration` tag (`AUTH_TTL_SECS = 300`).
- `nmp-core/src/publish/action.rs:142` — `PublishAction::PublishRaw { kind, tags, content, target, signer_pubkey }` is the kernel-signed generic publish.

**Artwork-upload contrast (confirmed):** `App/Sources/Agent/LiveAgentOwnedPodcastManager.swift:215-220` calls `kernel.blossomUpload(...)` — *"No private key in Swift; no URLSession in Swift."* It works because it signs with the **active roster account**. The Swift seam `KernelModel+BlossomUpload.swift:43-70` already accepts an optional `signerPubkeyHex` and forwards it as `signer_pubkey`. So the artwork path is the doctrine-correct mirror of what the per-podcast path should be.

---

### 2. NMP FFI/actor surface survey — what EXISTS vs MISSING

This is the surprise: **the "sign-as-non-active-account" capability the app header comment is waiting for already exists in the pinned rev.** It is not missing.

**EXISTS:**

- `nmp-core/src/actor/commands/identity.rs:787` — `sign_with_account_nonblocking(identity, pubkey, unsigned)`: *"Non-blocking sign with a SPECIFIC account, looked up by pubkey hex across BOTH the local-key and remote-signer maps — independent of which account is currently active."* It does `identity.keys.get(pubkey)` / `identity.remote_signers.get(pubkey)`.
- `nmp-core/src/publish/action.rs:142-156` — `PublishRaw.signer_pubkey: Option<String>`, documented at `:127-141`: *"`Some(hex_pubkey)` signs with the registered signer whose pubkey matches — e.g. an agent / per-podcast key added via `AddSigner { make_active: false }`. The active account is never changed."* The publish reducer (`publish.rs:135`) skips the active-account guard for `signer_pubkey: Some`.
- `nmp-blossom/src/action.rs:38-45` — `UploadInput.signer_pubkey`: *"`Some(hex)` = a named roster key (per-podcast NIP-F4)."* Resolved through the same `build_sign_event_for_account` port (`upload/mod.rs:163`).
- `nmp-ffi/src/identity.rs:103` — `nmp_app_signin_nsec(app, secret, make_active: u8)` → `AddSigner { LocalNsec, make_active: false }`. The reducer `add_signer` (`identity.rs:948-971`) calls `identity.add(keys)` which inserts into `identity.keys` + `order` and, when `make_active=false`, does NOT touch `identity.active`. The key is then immediately resolvable by `sign_with_account_nonblocking`.
- A non-active **bunker** agent precedent already exists: `nmp_app_signin_bunker(... make_active=0)` (`identity.rs:132`) is documented for *"agent/secondary keys that sign via `nmp_app_sign_event_for_return` without disturbing the user's active account."*

**MISSING (the two real gaps):**

- **(M1) Roster pollution / no hidden-account flag.** `sync_kernel` (`identity.rs:848-880`) builds the user-visible `AccountSummary` list by iterating `identity.order` unconditionally — every registered key, active or not, becomes a row in the account switcher. There is **no** `is_agent` / `hidden` / `app_managed` flag on `AccountSummary` or in `IdentityRuntime` (grep for `is_agent|hidden|app_managed` in `identity.rs`/`identity_state.rs` returns nothing). Adding a per-podcast nsec via `make_active:false` would surface N per-podcast keys in the user's account list.
- **(M2) No persistence for non-active local keys.** `session_persistence.rs:206` only persists via `enqueue_persist_current_active_session` (the **active** account). A per-podcast key added inactive would vanish on kernel restart — which is precisely why the app keeps its own persisted `PodcastKeyStore`. NMP has no "persist this non-active local key in the keyring and re-add it inactively on launch" path.

---

### 3. Recommendation: **Path (a) — per-podcast keys become first-class (but app-managed / hidden) NMP-registered signers.**

Rationale:
- The signing primitive (`sign_with_account_nonblocking`) and both consumption sites (`PublishRaw.signer_pubkey`, `nmp.blossom.upload.signer_pubkey`) already exist and are explicitly documented for this exact "per-podcast NIP-F4" use case. Path (a) lights up the existing seam; Path (b) would build a *parallel* sign-as-raw-key API alongside the roster one NMP already chose — duplicative and against NMP's own doctrine (the `PublishRaw` doc calls the roster-key path the intended design and references `app-signer-slot.md`).
- Path (b) (a raw-key signer that bypasses the roster) reintroduces "raw key bytes cross an API" — the D13 anti-pattern NMP deliberately avoids; the roster keeps keys inside the kernel.
- Path (a) deletes both `blossom.rs::upload_to_blossom` and `host_op_publish.rs::sign_event` and lets the app dispatch `nmp.publish { PublishRaw, signer_pubkey }` + `nmp.blossom.upload { signer_pubkey }` — removing all secp256k1/secret-bytes/BUD-02 code from the app.

**NMP changes Path (a) needs (exactly two):**
1. An "agent/hidden account" flag so an app-registered, non-active local key is registered + signable but **excluded from the user-visible account projection** (`AccountSummary`). This generalizes the existing non-active bunker-agent concept to local keys and to the projection layer.
2. Keyring persistence for non-active app-managed local keys (re-added inactively on launch), so the app can retire its `PodcastKeyStore`. (If NMP won't own persistence, the app could keep `PodcastKeyStore` purely as the seed source and re-register on launch — see verdict.)

Path (b) by contrast would need a whole new `sign-as-raw-key` port + wire variant on both publish and blossom; strictly more surface, against doctrine.

---

### 4. Is anything actionable in-repo NOW, without the NMP change?

**Yes — PARTIALLY ACTIONABLE.** The signing primitive is in the pinned rev today. A safe in-repo step exists that needs no NMP change, accepting one cosmetic regression:

The per-podcast publish + blossom upload can move off app-side `sign_event`/`upload_to_blossom` **today** by:
1. On `create_owned` / first publish, register the per-podcast key inactively: `nmp_app_signin_nsec(app, <podcast_nsec>, make_active=0)` (or the actor-internal `AddSigner { LocalNsec, make_active:false }`). FIFO ordering guarantees it lands before the dispatch.
2. Replace `publish_via_nmp(pre-signed)` with `publish_raw_via_nmp` extended to pass `signer_pubkey: Some(podcast_pubkey_hex)` (the `PublishRaw` variant already carries the field — `nmp_dispatch.rs:publish_raw_via_nmp` just needs to thread it through).
3. Replace `blossom::upload_to_blossom` with a dispatch of `nmp.blossom.upload { servers, file_path, signer_pubkey: Some(podcast_pubkey_hex) }`.

This deletes `blossom.rs` and `sign_event` and removes all raw-key crypto from the app **now**. The cost: the per-podcast keys would appear in the user-visible account switcher (gap M1), and the app must keep `PodcastKeyStore` as the seed-and-re-register source across launches (gap M2 — NMP won't persist them). So a *fully clean* retirement is upstream-gated on M1 (the hidden-account flag); a *crypto-out-of-the-app* retirement is shippable now if the roster-pollution cosmetic is acceptable as an interim.

Recommendation for sequencing: file the upstream issue (below) for the hidden-account + persistence support; the in-repo "lift signing into the kernel via inactive roster registration" step is a legitimate, doctrine-improving interim that removes the worst of the wart (raw keys + duplicated BUD-02) without waiting — gated only on product accepting temporary extra rows in the account list (or hiding them app-side in the Swift account UI as a stopgap).

---

### UPSTREAM NMP ISSUE DRAFT (review before filing — do NOT file yet)

**Title:** Support app-managed (hidden, non-active, persisted) local signing keys for per-app multi-identity use (per-podcast NIP-F4)

**Body:**

> **Context.** `PublishRaw.signer_pubkey` (`nmp-core/src/publish/action.rs:142`) and `nmp.blossom.upload.signer_pubkey` (`nmp-blossom/src/action.rs:38`) already let a non-active registered signer sign+publish/upload via `sign_with_account_nonblocking` (`nmp-core/src/actor/commands/identity.rs:787`), and `AddSigner { LocalNsec, make_active: false }` already registers a local key without activating it. The podcast app wants to retire its app-local secp256k1 signing + hand-rolled BUD-02 upload (`apps/nmp-app-podcast/src/blossom.rs`, `host_op_publish.rs::sign_event`) and route per-podcast NIP-F4 (kind:10154/54 + Blossom audio) entirely through these kernel seams. Two gaps block a clean retirement:
>
> **Gap 1 — Hidden / app-managed account flag.** `sync_kernel` (`identity.rs:848`) projects every entry in `IdentityRuntime.order` into the user-visible `AccountSummary` list. There is no flag to register a signer that is signable (resolvable by `sign_with_account_nonblocking`) but **excluded from the account-switcher projection**. Registering N per-podcast keys today would surface N spurious rows in the user's account list.
>
> **Request 1.** Add an "app-managed / agent / hidden" classification to a registered signer (local or remote) — e.g. `AddSigner { …, role: Agent }` or an `app_managed: bool` — such that:
> - the key is added to `identity.keys` / `remote_signers` and is resolvable by `sign_with_account_nonblocking` for `PublishRaw`/`blossom.upload`;
> - it is **never** included in the `AccountSummary` projection nor eligible as an active account (no accidental `SwitchActive` to it);
> - a corresponding FFI entry (e.g. `nmp_app_register_agent_nsec(app, secret)` or a `role` arg on `nmp_app_signin_nsec`).
>
> **Gap 2 — Persistence of non-active local keys.** `session_persistence.rs` (`:206` `enqueue_persist_current_active_session`) persists only the **active** local account. A per-podcast key added inactive is lost on kernel restart, forcing the app to keep its own `PodcastKeyStore`.
>
> **Request 2.** Persist app-managed/non-active local keys in the keyring and re-register them (inactively, hidden) on launch, so the app can delete its bespoke key store. (Alternatively, expose a documented contract that the app owns persistence and re-registers on each launch — but kernel-owned persistence is the D13-consistent choice.)
>
> **Why this and not a sign-as-raw-key API.** A raw-key sign port would reintroduce raw secret bytes crossing the API (D13) and duplicate the roster-key path NMP already chose (`PublishRaw`/`blossom` `signer_pubkey`). The roster-with-hidden-flag approach keeps keys inside the kernel and reuses the existing sign seam.
>
> **Acceptance.** With both: the podcast app dispatches `nmp.publish { PublishRaw, signer_pubkey }` and `nmp.blossom.upload { signer_pubkey }` for per-podcast keys, deletes `blossom.rs` + `host_op_publish.rs::sign_event` + `PodcastKeyStore`, holds zero secp256k1/secret bytes, and the per-podcast keys never appear in the user's account list.

---

### VERDICT

**PARTIALLY ACTIONABLE.**

- The "sign-as-non-active-account" API the app's own header comment (`host_op_publish.rs:8`) says it is waiting for is **already present** in the pinned NMP rev (`PublishRaw.signer_pubkey` + `nmp.blossom.upload.signer_pubkey` + `sign_with_account_nonblocking` + `AddSigner{make_active:false}`). That comment is stale.
- **Safe in-repo step this cycle:** register per-podcast keys via `AddSigner { LocalNsec, make_active:false }` and route per-podcast publish + Blossom through `PublishRaw { signer_pubkey }` / `nmp.blossom.upload { signer_pubkey }`, deleting `blossom.rs::upload_to_blossom` and `host_op_publish.rs::sign_event`. This removes ALL raw-key crypto and duplicated BUD-02 from the app now.
- **Upstream-gated for a *fully* clean retirement:** the hidden-account flag (M1, else per-podcast keys pollute the account switcher) and non-active-key persistence (M2, else `PodcastKeyStore` must remain as seed/re-register source). File the issue above for those.

So: not fully blocked. The largest part of the wart (app-held secp256k1 keys + hand-rolled signing + hand-rolled BUD-02) is removable in-repo immediately; only the cosmetic roster-pollution and the `PodcastKeyStore`'s continued existence wait on the upstream issue.
