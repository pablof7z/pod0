# Generated Pod0 core bindings

`Swift/` and `Kotlin/` are generated from the exact Rust facade metadata and
are committed so review and API drift are visible. Never edit generated source
or headers by hand.

Regenerate with:

```sh
./scripts/generate_core_bindings.sh
```

CI runs `check_core_binding_drift.sh` and fails if regeneration differs. The
generation script normalizes trailing whitespace and end-of-file newlines after
UniFFI emission. Generated files are exempt from hand-authored ownership and
line-length rules; their Rust source types remain subject to those checks.
