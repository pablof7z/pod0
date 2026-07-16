# NMP foundation integration

Pod0's app-owned boundary is `Pod0NMPEngineAccess`. The sole production owner
is `Pod0NMPComposition`; product slices receive that dependency and must not
construct another `NMPEngine`. `Pod0NMPConfiguration` is immutable for an
engine lifetime. A relay edit calls `stageOperatorRelay` and becomes effective
only at the next controlled construction.

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

`Pod0HumanIdentityLifecycle` therefore reports
`clientInitiatedNip46CheckpointUnsupported(issue: 571)`. M1 cannot close until
`pablof7z/nmp#571` provides and proves secure checkpoint and cold-start restore
for new client-initiated sessions.

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
