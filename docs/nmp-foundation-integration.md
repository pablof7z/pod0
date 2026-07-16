# NMP foundation integration

Pod0's app-owned boundary is `Pod0NMPEngineAccess`. The sole production owner
is `Pod0NMPComposition`; product slices receive that dependency and must not
construct another `NMPEngine`. `Pod0NMPConfiguration` is immutable for an
engine lifetime. A relay edit persists the next-construction operator policy;
it neither mutates nor overlaps the current engine. The settings UI states that
the changed value becomes effective after Podcastr fully closes and relaunches.

The selected source baseline is
`317b7caaf5a83da1e6899efcc5aeb90a85b808c3`. It must match the
`Vendor/nmp` gitlink, `Vendor/nmp-revision.txt`, and
`Pod0NMPBuild.testedRevision`; runtime code never resolves a branch.

## Launch integration

After the pinned NMP package is available to the app target, app composition:

1. creates and prepares `Pod0NMPStoreLayout.applicationSupport()`;
2. constructs one `Pod0NMPComposition` and retains it for the process lifetime;
3. constructs `NMPEngine` with NMP's public `NMPKeychainAccountStore` in the
   new `<bundle>.nmp-human-identity` namespace, so NMP alone loads and restores
   its checkpoint before the composition is exposed;
4. creates or loads the clean-start, non-secret `Pod0IdentityCatalog` from
   Keychain and verifies its expected public key against `activeAccount()`;
5. retains `NMPAccountRegistration` only for accounts imported in this
   process, where NMP's exact removal handle is actually available;
6. fails human profile, note, and clip publication before signing or enqueue
   because the pinned engine cannot yet prove crash-safe publish correlation.

The foundation does not translate or import state from an older Nostr owner.
Product data already owned by Pod0 remains outside this boundary and is not
modified by NMP setup.

`UserIdentityStore` is only the observable UI projection. It never reads old
identity namespaces, constructs `LocalKeySigner`, or starts an app-owned
NIP-46 transport. The retired remote transport and crypto files are not in the
app target. Nonhuman agent signing lives separately under `App/Sources/Agent`.

Raw WebSocket profile reads are removed. Profile discovery is simply not
enabled in this milestone; this is not an NMP capability claim. The dormant
feedback feature and its package dependency are removed.

No scene-phase callback, reconnect timer, subscription replay, polling loop,
or settings observer constructs or repairs NMP.

## Backup and reset decision record

The canonical store is exactly
`Application Support/podcastr/nmp/canonical.redb`. Its root directory uses
`NSFileProtectionCompleteUntilFirstUserAuthentication`: it is unavailable until
the first device unlock after boot, then remains usable for background work.
The root is excluded from device and cloud backup because canonical events and
source evidence are reacquired by NMP, while identity secrets remain in
Keychain.

Pod0 owns four separate policies:

| Operation | AppState | NMP store and durable writes | Receipt annotations | Keychain |
| --- | --- | --- | --- | --- |
| Cache-preserving sign-out | Preserve | Preserve, including pending writes | Preserve | Detach only the exact active human identity |
| Clear app data, preserve identities | Clear product data; preserve settings | Preserve | Clear | Preserve all current identities and credentials |
| Reset Nostr data | Preserve | Reset only after engine shutdown | Clear | Preserve all current identities and credentials |
| Mutually-untrusted-user handoff | Clear all | Reset only after engine shutdown | Clear | Clear all current Pod0 identities and credentials |

Ordinary sign-out never cancels or retargets an accepted durable write. Nostr
store reset and untrusted-user handoff require distinct explicit confirmation
values; they are not aliases for the existing Clear All Data alert. Reset first
shuts down the composition. If deletion fails, the composition stays shut down
and the same idempotent reset can be retried before dependent Pod0 data is
cleared. A successful reset requires a fresh app composition before Nostr or
identity features resume.

## Exact upstream blocker

The selected revision accepts caller-supplied secret material through
`NMPEngine.addAccount(secretKey:)`, but exposes no NMP-owned API for generating
a new local account. Pod0 does not fill that gap with app-side key generation.
A clean installation therefore fails closed with an actionable import-or-bunker
message until `pablof7z/nmp#588` provides secure NMP-owned generation.

As of the selected revision, Swift `NMPNip46Connection` supports a live
`bunkerURI` or in-memory invitation. It exposes neither secure export/checkpoint
nor cold-start restoration of a newly created client-initiated invitation
session. Pod0 does not add a private checkpoint or transport alongside NMP.

The identity UI therefore offers only pasted bunker links; it does not expose
or persist a speculative client-initiated state. Scan-to-connect remains
tracked by `pablof7z/nmp#571` until secure checkpoint and cold-start restore are
available.

The selected revision also does not expose the exact registration handle for a
local account restored during engine initialization. Pod0 therefore blocks
sign-out and identity switching for that cold-restored account without changing
UI, catalog, active account, or checkpoint. This is tracked by
`pablof7z/nmp#589`. A same-process import retains its exact registration and
can sign out through `removeAccount(_:)`.

The selected revision cannot durably correlate a human publication across a
crash without an app-owned receipt journal. Pod0 does not add that parallel
coordinator. Profile editing and all human profile, note, and clip publication
therefore stop before signing or enqueue. This is tracked by
`pablof7z/nmp#591`.
