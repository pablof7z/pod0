# Activity conformance inventory

This directory is the executable scope ledger for ADR-0009 and epic #204. It
records every current typed product ingress and every source module detected as
a recovery, authoritative mutation, or native execution surface.

- `commands.json` maps every `ApplicationCommand` variant.
- `host-requests.json` maps every `HostRequest` variant.
- `host-observations.json` maps every `HostObservation` variant.
- `surfaces.json` registers recovery paths, current authoritative mutation
  modules, and native execution modules.

`scripts/check_activity_conformance.py` compares the manifests to source. A
new enum variant or detected surface fails until it has an explicit Rust owner,
fact policy, child issue, and migration status.

An inventory row is not proof that the target architecture is implemented.
`implementation_status` deliberately distinguishes registered legacy/current
surfaces from completed enforcement. Issue #219 may close only when every row
uses a verified terminal status, the direct-mutator and native-policy exception
sets are empty, and the required crash/bypass suites pass.
