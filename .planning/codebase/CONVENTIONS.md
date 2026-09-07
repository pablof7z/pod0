# Coding Conventions

**Analysis Date:** 2026-08-22

## Naming Patterns

**Files:**
- Modules and implementations: `snake_case.rs` (e.g., `effect_outbox.rs`, `listening_store.rs`)
- Test modules: `{name}_tests.rs` (e.g., `effect_outbox_tests.rs`, `library_store_tests.rs`)
- Error types: `{domain}_error.rs` (e.g., `listening_error.rs`)
- Models/schemas: `{domain}_model.rs` (e.g., `effect_outbox_model.rs`)

**Functions and Methods:**
- `snake_case` for all functions and methods (e.g., `claim_next`, `apply_feed`, `open_authoritative`)
- Constructor methods: `open`, `new` (e.g., `LibraryStore::open_authoritative`)
- Builder/query methods: descriptive verbs with `_with_` for variants (e.g., `claim_next_with_identity`)

**Variables:**
- `snake_case` for all local variables, parameters, and struct fields
- Constants: `UPPER_SNAKE_CASE` (e.g., `MIN_LEASE_MILLISECONDS`, `CURRENT_SCHEMA_VERSION`)

**Types:**
- `PascalCase` for structs, enums, and traits (e.g., `EffectLease`, `StorageError`)
- Error enums: `{Domain}Error` (e.g., `ListeningDomainError`, `EffectOutboxError`)

## Code Style

**Formatting:**
- Rust 2024 edition
- Rust 1.93+ required (see `rust/rust-toolchain.toml`)
- No explicit formatter config; uses Rust defaults
- Line wrapping: multi-line imports for clarity; long method chains allowed

**Linting:**
- Clippy enabled via `./scripts/check_rust.sh`
- No tolerance for unsafe code: `#![forbid(unsafe_code)]` at crate level
- All public API surfaces checked via facade/binding generation

**Safety:**
- `#![forbid(unsafe_code)]` declared in all crates (`pod0-domain`, `pod0-storage`, `pod0-facade`, etc.)
- Error handling through `Result<T, E>` with explicit error types

## Import Organization

**Order:**
1. Standard library: `use std::...`
2. External crates: `use {external_crate}::{items}`
3. Workspace crates: `use pod0_domain::...`, `use pod0_application::...`, `use pod0_storage::...`
4. Internal modules: `use crate::{path}::{items}`
5. Conditional/test imports: `use crate::{test_module}` within `#[cfg(test)]` blocks

**Style:**
- Multi-line imports for grouped related items (e.g., many types from one module)
- Grouped by source, not alphabetized within groups
- Example from `facade_exports.rs`:
  ```rust
  pub use pod0_application::{
      AdSpanProjection, AgentApprovalDecision, ...
  };
  pub use pod0_domain::{
      AdSpanEvaluation, AdSpanId, ...
  };
  ```

**Re-exports:**
- Barrel files use `pub use {module}::*` pattern
- `lib.rs` declares all modules via `mod` then re-exports via `pub use`

## Error Handling

**Error Types:**
- Use explicit enum variants for each error case, no generic error codes
- Enum variants are descriptive nouns: `Storage`, `InvalidLeaseDuration`, `StaleLease`
- Example from `effect_outbox_model.rs`:
  ```rust
  pub enum EffectOutboxError {
      Storage,
      InvalidLeaseDuration,
      StaleLease,
      InvalidRecord,
  }
  ```

**Error Traits:**
- Implement `std::fmt::Display` for public errors with detailed messages
- Implement `std::error::Error` trait
- Example from `listening_error.rs`: each variant maps to a human-readable message explaining the invariant violation

**Error Propagation:**
- Use `Result<T, E>` everywhere; no panics in library code
- Domain/application layer errors use `uniffi::Error` derive for FFI
- Storage layer errors use custom Display implementations for detailed diagnostics

## Comments

**When to Comment:**
- Doc comments (`///`) on types, fields, and public functions when the invariant is non-obvious
- Example: `/// Versioned comparison identity matching the current Swift store exactly: lowercase the complete absolute URL without trimming a trailing slash.`
- Line comments for subtle algorithm invariants or cross-layer constraints
- No comments for obvious code; names should be self-documenting

**Doc Comments Style:**
- Multi-line doc comments allowed when explaining complex invariants
- Link to related concepts or constraints (e.g., "This domain boundary resolves X during Y")

## Derive Macros

**Standard derives (all structs):**
- `Clone` — always included
- `Debug` — always included
- `PartialEq`, `Eq` — for domain types and models
- `Copy` — for small value types (IDs, enums)

**FFI derives:**
- `uniffi::Record` — for types exported to native bindings
- `uniffi::Enum` — for enums exported to bindings
- `uniffi::Error` — for public error types

**Serialization:**
- `serde::Serialize`, `serde::Deserialize` — for types persisted or transmitted (IDs, records)

**Example from `listening.rs`:**
```rust
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FeedIdentityV1 {
    pub source_url: String,
    pub comparison_key: String,
}
```

## Module Design

**Exports:**
- `pub use` declarations in `lib.rs` for public API
- Single responsibility: one major type/domain per module
- Related support modules: `{name}_model`, `{name}_codec`, `{name}_read`, `{name}_write`

**File Organization in Storage Crate:**
- `lib.rs` declares 100+ modules in order of dependency
- Tests in separate `#[cfg(test)] mod {name}_tests` blocks
- No test code mixed with implementation

**Const Functions:**
- Mark small constructors and accessors as `#[must_use] pub const fn`
- Example: ID construction (`from_parts`, `from_bytes`) and conversions

## ID Types

**Pattern:**
- All cross-layer IDs use the `opaque_id!` macro in `pod0-domain/src/lib.rs`
- Two `u64` fields: `high` and `low`
- Serializable: derive `serde::Serialize`, `serde::Deserialize`
- Comparable: `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`
- Conversion methods: `from_parts(high, low)`, `from_bytes([u8; 16])`, `into_bytes()`
- No string representation; meaning remains domain-specific

## Architectural Constraints

**Unsafe code:** Forbidden everywhere via `#![forbid(unsafe_code)]`

**Dependencies:**
- Workspace crates define shared versions centrally in root `Cargo.toml` under `[workspace.dependencies]`
- All crates inherit workspace `version`, `edition`, `rust-version`, `license`

**Module visibility:**
- Private by default; only `pub` what crosses layer boundaries
- Internal implementation details stay `pub(crate)`

---

*Convention analysis: 2026-08-22*
