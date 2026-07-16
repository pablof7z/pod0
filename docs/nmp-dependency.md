# NMP source dependency

Podcastr consumes NMP from the `Vendor/nmp` Git submodule. The gitlink and
`Vendor/nmp-revision.txt` must name the same full commit. CI logs that revision
and builds generated Swift bindings plus `NMP.xcframework` from that checkout;
generated bindings and binaries remain ignored build output.

`ci_scripts/bootstrap_project.sh` performs the complete clean-checkout path.
Its default `NMP_BUILD_MODE=sim-only` produces arm64 and x86_64 simulator
slices plus the macOS host slice used by SwiftPM. TestFlight sets
`NMP_BUILD_MODE=all`, which adds the arm64 physical-device slice used by the
archive. The build uses the exact Rust toolchain declared in the bootstrap
script rather than following a moving nightly.

## Updating NMP deliberately

1. Choose a reviewed NMP commit; never select a branch name or implicit HEAD.
2. Run `git -C Vendor/nmp fetch origin` and check out that commit detached.
3. Put the same 40-character revision in `Vendor/nmp-revision.txt`.
4. Stage both paths: `git add Vendor/nmp Vendor/nmp-revision.txt`.
5. Run `ci_scripts/verify_repository_dependencies.sh`.
6. Run `NMP_BUILD_MODE=sim-only ci_scripts/bootstrap_project.sh`, followed by
   `ci_scripts/run_tests.sh`.
7. Before release, run `NMP_BUILD_MODE=all ci_scripts/bootstrap_project.sh`
   and qualify the archive so its device slice is proven from the same pin.

If NMP changes its minimum Rust toolchain, review and update the exact dated
toolchain in `bootstrap_project.sh` in the same change. Do not commit
`Vendor/nmp/Packages/NMP/NMP.xcframework`, generated `NMPFFI` sources, or Cargo
build output.
