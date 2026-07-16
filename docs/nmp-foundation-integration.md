# NMP foundation integration

Pod0's app-owned boundary is `Pod0NMPEngineAccess`. The sole production owner
is `Pod0NMPComposition`; product slices receive that dependency and must not
construct another `NMPEngine`. `Pod0NMPConfiguration` is immutable for an
engine lifetime. A relay edit calls `stageOperatorRelay` and becomes effective
only at the next controlled construction.

The selected source baseline is
`867aecfd83aad47a3ec31ff07f0c564505da0eef`. Bootstrap may replace
`Pod0NMPBuild.testedRevision` from repository-contained generated pin metadata,
but runtime code must never resolve a branch.

## Launch integration

After the pinned NMP package is available to the app target, app composition:

1. creates and prepares `Pod0NMPStoreLayout.applicationSupport()`;
2. runs `NMPInstalledStateMigration.prepareIfNeeded`, persists the returned
   active state, and keeps the fail-closed legacy ingress flag effective;
3. constructs one `Pod0NMPComposition` and retains it for the process lifetime;
4. loads/migrates `Pod0IdentityCatalog` from Keychain;
5. constructs `Pod0HumanIdentityLifecycle`, stops any legacy remote signer,
   and restores only the catalog's human entry;
6. retains and renders the composition's pushed diagnostics stream.

No scene-phase callback, reconnect timer, subscription replay, polling loop,
or settings observer constructs or repairs NMP.

## Exact upstream blocker

As of the selected revision, Swift `NMPNip46Connection` supports a live
`bunkerURI` or in-memory invitation. It exposes neither secure export/checkpoint
nor import/restore of a client-initiated invitation session. Pod0's legacy
`nostrconnect` schema contains the client session key, but inventing a private
NMP import door would violate ownership and cannot prove cold-start continuity.

`Pod0HumanIdentityLifecycle` therefore reports
`clientInitiatedNip46RestoreUnsupported(issue: 571)` and does not start either
NMP or the legacy transport for that identity. M1 cannot close until
`pablof7z/nmp#571` provides and proves secure checkpoint/import. A legacy remote
record without a secret is also treated as client-initiated/ambiguous rather
than guessed to be a reconnectable bunker.

