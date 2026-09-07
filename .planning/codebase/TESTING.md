# Testing Patterns

**Analysis Date:** 2026-08-22

## Test Framework

**Runner:**
- Built-in Rust test framework via `cargo test`
- No external test runner; uses `#[test]` attribute
- Config: `rust/Cargo.toml` workspace with 815 test functions across 246 test modules

**Run Commands:**
```bash
cargo test                          # Run all tests in workspace
cargo test -p {crate-name}          # Run tests for specific crate
cargo test --lib                    # Run library tests only (exclude integration tests)
./scripts/check_rust.sh             # Full reproducible checks including tests, Clippy, formatting
```

**Assertion Library:**
- Standard Rust macros: `assert!`, `assert_eq!`, `assert_ne!`
- Pattern matching: `assert!(matches!(value, pattern))`
- Option/Result: `unwrap()`, `is_ok()`, `is_err()`

## Test File Organization

**Location:**
- Tests co-located in separate `*_tests.rs` files, not in implementation files
- Located in same directory as the module being tested
- Example: `effect_outbox.rs` has corresponding `effect_outbox_tests.rs`

**Naming:**
- `{module_name}_tests.rs` for all test files
- 246 test modules found: `library_store_tests.rs`, `listening_completion_tests.rs`, etc.

**Structure:**
- Declared in `lib.rs` with `#[cfg(test)] mod {name}_tests;`
- Tests use `use super::*;` to import the module under test
- Helper functions and fixtures defined in test module

## Test Structure

**Suite Organization:**
```rust
#[test]
fn descriptive_test_name_describing_behavior() {
    let fixture = imported_fixture();
    let result = store.method(fixture, args);
    assert_eq!(result, expected);
}
```

**Patterns:**

**Setup - Fixtures:**
- Helper functions create test fixtures: `imported_fixture()`, `golden_snapshot()`
- Fixtures from support modules: `use crate::listening_import_test_support::*;`
- Example from `library_store_tests.rs`:
  ```rust
  let fixture = imported_fixture();
  commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
  let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
  ```

**Teardown:**
- Automatic: filesystem fixtures cleaned up when `Fixture` drops
- No explicit cleanup needed in most cases

**Assertion Pattern:**
- Test state by reading snapshots and comparing fields
- Use `assert_eq!` for exact matches, `assert!` with predicates for complex conditions
- Example from `listening_completion_tests.rs`:
  ```rust
  assert!(matches!(
      validate_listening_snapshot(snapshot.clone()),
      Err(ListeningDomainError::CompletedEpisodeHasResumePosition)
  ));
  ```

**Idempotency Testing:**
- Common pattern: replay same operation, assert same result
- Example from `library_store_tests.rs`:
  ```rust
  let first = store.upsert_external_episode(...).unwrap();
  assert_eq!(
      store.upsert_external_episode(...).unwrap(),
      first
  );
  ```

## Mocking

**Framework:** Not used — integration testing with real state

**Philosophy:**
- No mock databases; tests use real SQLite instances via `tempfile` crate
- No mock time; tests pass explicit `UnixTimestampMilliseconds` values
- No mock models; tests use real domain types

**What NOT to Mock:**
- Storage layers — always use real database
- Domain logic — test with real types and values
- Time-dependent behavior — pass explicit timestamps

**Test Support Modules:**
- `recovery_test_support.rs` provides `Fixture` for temp database creation
- `listening_import_test_support.rs` provides fixture builders for listening domain
- Support modules public within `#[cfg(test)]` scope

## Fixtures and Factories

**Test Data:**
```rust
fn imported_fixture() -> Fixture {
    // Creates temporary directory with pre-populated database
    // Returns Fixture { target: PathBuf, ... }
}

fn golden_snapshot() -> ListeningDomainSnapshot {
    // Returns canonical test snapshot with known state
}

fn refreshed_podcast(podcast_id: PodcastId) -> PodcastRecord {
    // Factory returning podcast with test data
}
```

**Location:**
- Support modules: `{domain}_test_support.rs` (e.g., `listening_import_test_support.rs`, `evidence_store_test_support.rs`)
- Declared in `lib.rs` as `#[cfg(test)] mod {name}_test_support;`
- Imported via `use crate::{module}::*;` in test modules

**Fixture Scope:**
- `Fixture` struct holds temporary directory path
- Cleanup automatic on drop via `Drop` impl
- Safe for parallel test execution (each test gets isolated temp dir)

## Coverage

**Requirements:** None enforced; all tests must pass but no coverage target

**Test Scope:**
- 815 test functions across workspace
- Heavy integration testing with state verification
- Tests exercise database transactions, state transitions, idempotency, and invariant validation

**Critical Areas Tested:**
- Domain invariants: `validate_listening_snapshot()` tests completeness and consistency
- State transitions: cutover markers, schema migrations
- External effect management: leasing, expiry, fence validation
- Feed updates and subscriptions: upsert atomicity and idempotency
- Query correctness: snapshot state matching expected results

## Test Types

**Unit Tests:**
- Scope: Single function or small method group
- Approach: Pass known inputs, assert specific outputs
- Examples: error enum variant matching, ID conversion roundtrips

**Integration Tests:**
- Scope: Full workflow involving multiple modules
- Approach: Set up fixture, invoke complex operation sequence, verify final state
- Examples: `cutover_is_atomic_idempotent_and_required_for_runtime_writes()` spans transition/activity/listening stores

**Snapshot/State Tests:**
- Scope: Verify system state after operation
- Approach: Load snapshot, mutate, validate consistency invariants
- Examples: completeness checks in `listening_completion_tests.rs`

**Idempotency Tests:**
- Scope: Same operation run multiple times yields same result
- Approach: Replay operation with same inputs at different timestamps, assert result matches
- Common in library store tests for feed updates and external episodes

## Common Patterns

**Async Testing:**
- Not used; no async test framework
- All blocking calls: `rusqlite::Connection`, database operations, file I/O

**Error Testing:**
```rust
assert!(matches!(
    LibraryStore::open_authoritative(&fixture.target),
    Err(crate::StorageError::CutoverNotAuthoritative)
));
```

**State Mutation Testing:**
```rust
let mut snapshot = golden_snapshot();
snapshot.episodes[0].listening.completion = CompletionStatus::Completed {
    cause: CompletionCause::NaturalEnd,
};
snapshot.episodes[0].listening.resume_position_milliseconds = 0;
assert!(validate_listening_snapshot(snapshot).is_ok());
```

**Fixture Reuse:**
```rust
let fixture = imported_fixture();
commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
// Multiple test operations on same store
```

---

*Testing analysis: 2026-08-22*
