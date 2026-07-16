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
3. creates or loads the clean-start `Pod0IdentityCatalog` from Keychain;
4. constructs `Pod0HumanIdentityLifecycle` with NMP's public
   `NMPKeychainAccountStore` in the new `<bundle>.nmp-human-identity`
   namespace, loads the selected secret
   exactly once, and registers it through that composition's engine;
5. retains NMP's opaque `NMPAccountRegistration` so sign-out removes the
   exact capability even after a cold launch;
6. signs and publishes product events directly through `NMPEngine.signEvent`
   and `NMPEngine.publish`, with no signer or relay-transport adapter;
7. retains and renders the composition's pushed diagnostics stream.

The foundation does not translate or import state from an older Nostr owner.
Product data already owned by Pod0 remains outside this boundary and is not
modified by NMP setup.

`UserIdentityStore` is now only the observable UI projection. It never reads
its former Keychain namespaces, constructs `LocalKeySigner`, or starts the old
`RemoteSigner` transport. Local material crosses into NMP once per process;
subsequent product signing uses the one composition's active NMP capability.

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
