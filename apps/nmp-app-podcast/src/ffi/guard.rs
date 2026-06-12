//! FFI entry-point panic guard.
//!
//! Every `#[no_mangle] extern "C"` and `extern "system"` symbol in this crate
//! MUST wrap its body in [`ffi_guard`].  Without this, a panic inside any of
//! those bodies would unwind across the C ABI boundary — undefined behaviour on
//! `rustc < 1.81` and a deterministic process abort on `rustc >= 1.81` — where
//! returning an error sentinel would have been possible.
//!
//! ## Why `AssertUnwindSafe` is sound here
//!
//! The typical call site pattern is:
//!
//! ```text
//! pub extern "C" fn foo(handle: *mut PodcastHandle, ...) -> *mut c_char {
//!     ffi_guard("foo", std::ptr::null_mut(), || {
//!         let handle = unsafe { &*handle };  // null-checked before the guard
//!         …
//!     })
//! }
//! ```
//!
//! Raw pointers and `&PodcastHandle` are `!UnwindSafe` by default.  Asserting
//! is sound here because:
//!
//! 1. All pointer arguments are null-checked **before** the closure is
//!    constructed, so only guaranteed-non-null pointers enter the closure.
//! 2. On the panic path the closure's captured values are **never observed
//!    again** — the caller returns the fallback sentinel immediately; no
//!    partially-mutated shared state is re-read after the unwind.
//! 3. The `PodcastHandle` internals are `Arc<Mutex<…>>`; a poison-flagged
//!    mutex is already tolerated everywhere in this codebase (callers treat
//!    `Err(_)` from `.lock()` as a D6 silent degrade), so a poisoned mutex
//!    after a mid-body panic is handled correctly by all callsites.
//!
//! ## Relationship to nmp-core
//!
//! `nmp-core` ships `guard_ffi_callback` (returning `Option<R>`) for guarding
//! *callback fan-out* sites where `None` is meaningful.  This helper guards
//! the *entry-point* side of the ABI where the caller expects a concrete
//! fallback value (null ptr, 0, false, `{"error":…}` JSON) rather than
//! `Option`.  The two helpers are complementary, not redundant.
//!
//! ## `panic="abort"` is deliberately NOT set
//!
//! Setting `panic="abort"` would nullify both this guard and nmp-core's own
//! actor-tick `catch_unwind`.  This crate MUST compile with `panic="unwind"`
//! (the default) for `catch_unwind` to have any effect.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Invoke `body` inside a [`catch_unwind`] so a panic cannot unwind across the
/// C ABI boundary.
///
/// `site` is a short label identifying the call site. On a caught panic it is
/// emitted via `log::error!` so the failure is not completely silent (the only
/// observability hook for a contained ABI-boundary panic); the success path
/// stays side-effect free (D6: failures are data, not host-visible behaviour).
///
/// `fallback` is a **closure** that produces the sentinel the function already
/// returns for a hard error (e.g. `|| null_mut()`, `|| 0`, `|| false`, or the
/// existing `|| {"error":"…"}` envelope).  It is invoked **only on the panic
/// path** — this laziness matters: an eagerly-evaluated allocating fallback
/// (e.g. `err_envelope(...).into_raw()`) would leak its heap `CString` on every
/// successful call, because `ffi_guard` returns `body`'s value and silently
/// drops the unused raw-pointer fallback (dropping a `*mut c_char` is a no-op).
/// Do NOT invent new sentinels.
///
/// # Soundness note
///
/// [`AssertUnwindSafe`] is required because raw pointers and most FFI types are
/// `!UnwindSafe` by default.  See the module-level doc for why this is sound at
/// every call site in this crate.
#[inline]
pub(crate) fn ffi_guard<T>(
    site: &str,
    fallback: impl FnOnce() -> T,
    body: impl FnOnce() -> T,
) -> T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(val) => val,
        Err(_) => {
            // Sole observability hook for a contained ABI-boundary panic.
            // Cheap, lazy-formatted, and never on the success path.
            log::error!("ffi_guard: caught panic at {site}; returning fallback sentinel");
            fallback()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_char;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::ffi_guard;

    // ── Basic contract ──────────────────────────────────────────────────────

    #[test]
    fn normal_return_passes_through() {
        let result = ffi_guard("test_site", || 0i32, || 42i32);
        assert_eq!(result, 42);
    }

    #[test]
    fn panic_returns_fallback_not_abort() {
        // THE KEY INVARIANT: a panic in `body` must NOT propagate.
        // If `catch_unwind` failed to contain it, this test would abort the
        // process (the test runner would crash) rather than complete.
        let result = ffi_guard("inject_panic", || -1i32, || -> i32 {
            panic!("injected panic — process must not abort");
        });
        // Process did not abort; fallback was returned.
        assert_eq!(result, -1);
    }

    #[test]
    fn null_ptr_fallback_for_string_return() {
        // Representative: functions returning `*mut c_char` use `null_mut()`.
        let result: *mut c_char = ffi_guard("ptr_test", ptr::null_mut, || -> *mut c_char {
            panic!("null-ptr fallback test");
        });
        assert!(result.is_null());
    }

    #[test]
    fn false_fallback_for_bool_return() {
        let result: bool = ffi_guard("bool_test", || false, || -> bool {
            panic!("bool fallback test");
        });
        assert!(!result);
    }

    #[test]
    fn zero_fallback_for_scalar_return() {
        let result: u64 = ffi_guard("u64_test", || 0u64, || -> u64 {
            panic!("scalar fallback test");
        });
        assert_eq!(result, 0u64);
    }

    #[test]
    fn loop_across_mix_of_panic_and_normal() {
        // Multiple calls — panicking iteration does NOT abort; others succeed.
        for i in 0i32..4 {
            let result = ffi_guard("loop_site", || -1i32, || {
                if i == 2 {
                    panic!("iteration 2 panics");
                }
                i
            });
            if i == 2 {
                assert_eq!(result, -1, "iteration {i}: expected fallback");
            } else {
                assert_eq!(result, i, "iteration {i}: expected value");
            }
        }
    }

    // ── Lazy-fallback / no-leak guarantee ───────────────────────────────────

    /// THE REGRESSION GUARD for the #387 memory leak: on the success path the
    /// fallback closure MUST NOT be constructed/invoked. For the ~18
    /// string-returning entries whose fallback allocates a `CString` and leaks
    /// it via `into_raw()`, eager evaluation leaked that heap allocation on
    /// every successful call. A lazy fallback closure is the fix; this test
    /// directly proves the closure never runs when `body` returns `Ok`.
    #[test]
    fn success_path_never_invokes_fallback_closure() {
        static FALLBACK_INVOKED: AtomicBool = AtomicBool::new(false);
        FALLBACK_INVOKED.store(false, Ordering::SeqCst);

        let result = ffi_guard(
            "success_no_leak",
            || {
                // Simulates the allocating fallback (e.g.
                // `err_envelope("panic").into_raw()`). If this runs on the
                // success path, the allocation leaks.
                FALLBACK_INVOKED.store(true, Ordering::SeqCst);
                7i32
            },
            || 42i32,
        );

        assert_eq!(result, 42, "success value must pass through");
        assert!(
            !FALLBACK_INVOKED.load(Ordering::SeqCst),
            "fallback closure was invoked on the SUCCESS path — leak regression!"
        );
    }

    /// Complementary: on the panic path the fallback closure IS invoked exactly
    /// once, and its value is returned.
    #[test]
    fn panic_path_invokes_fallback_closure_exactly_once() {
        static FALLBACK_COUNT: AtomicUsize = AtomicUsize::new(0);
        FALLBACK_COUNT.store(0, Ordering::SeqCst);

        let result = ffi_guard(
            "panic_invokes_once",
            || {
                FALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
                -1i32
            },
            || -> i32 { panic!("force fallback") },
        );

        assert_eq!(result, -1, "fallback value must be returned");
        assert_eq!(
            FALLBACK_COUNT.load(Ordering::SeqCst),
            1,
            "fallback closure must run exactly once on the panic path"
        );
    }

    // ── Representative real-entry regression tests ──────────────────────────

    /// Mirrors the success path of `nmp_app_podcast_snapshot_rev`: the guard
    /// passes through the correct `u64` value when no panic occurs.
    #[test]
    fn regression_snapshot_rev_success_path() {
        let rev: u64 = 42;
        let result = ffi_guard("nmp_app_podcast_snapshot_rev", || 0u64, || rev);
        assert_eq!(result, 42u64);
    }

    /// Mirrors the null-handle degrade path: the guard is a no-op when the
    /// body short-circuits with the existing sentinel.
    #[test]
    fn regression_snapshot_rev_null_handle_path() {
        let result = ffi_guard("nmp_app_podcast_snapshot_rev", || 0u64, || {
            let handle_is_null = true;
            if handle_is_null {
                return 0u64; // existing null-handle degrade sentinel
            }
            42u64
        });
        assert_eq!(result, 0u64);
    }

    /// Inject a panic into a test-only `#[no_mangle] extern "C"` entry and
    /// assert the fallback sentinel is returned and the process does NOT abort.
    /// This is the injected-panic test required by the task specification.
    #[test]
    fn injected_panic_returns_null_ptr_sentinel_and_process_does_not_abort() {
        #[no_mangle]
        extern "C" fn nmp_app_ffi_guard_test_panic_injection() -> *mut c_char {
            ffi_guard(
                "nmp_app_ffi_guard_test_panic_injection",
                ptr::null_mut,
                || -> *mut c_char { panic!("test: injected panic in extern C body") },
            )
        }
        // Call the extern "C" function — if the process aborts, this test fails.
        let result = nmp_app_ffi_guard_test_panic_injection();
        assert!(result.is_null(), "expected null fallback, got non-null");
        // Reaching here proves the process did NOT abort.
    }
}
