//! Thin wrappers over the NMP + Podcast FFI surface.
//!
//! Raw pointer operations are isolated here. All unsafe blocks are explicit
//! and justified by caller contract comments.

use std::ffi::{CStr, CString};
use std::time::{Duration, Instant};

use nmp_app_podcast::{
    nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free,
    nmp_app_podcast_snapshot_rev, PodcastHandle,
};
use nmp_app_podcast::ffi::PodcastUpdate;
use nmp_ffi::{nmp_app_dispatch_action, nmp_app_free_string, nmp_app_new};
use nmp_ffi::NmpApp;

/// Allocate a new `NmpApp` instance. Panics if the kernel returns null
/// (should never happen in practice — null only comes from OOM).
pub fn app_new() -> *mut NmpApp {
    let app = nmp_app_new();
    assert!(!app.is_null(), "nmp_app_new returned null");
    app
}

/// Free a previously-allocated `NmpApp`. The actor thread is joined first
/// (that happens inside `NmpApp::drop`).
///
/// # Safety
/// `app` must be a valid pointer returned by `nmp_app_new` and not yet freed.
pub unsafe fn app_free(app: *mut NmpApp) {
    if !app.is_null() {
        // SAFETY: caller guarantees this pointer came from `nmp_app_new` and
        // is freed exactly once. `Box::from_raw` reclaims the heap allocation;
        // `Drop` joins the actor thread before releasing the memory.
        drop(unsafe { Box::from_raw(app) });
    }
}

/// Dispatch a JSON action to the kernel and return the decoded result value.
///
/// The `namespace` / `payload` shape must match the registered `ActionModule`
/// for that namespace. Returns `serde_json::Value::Null` if the returned C
/// string is empty or not valid UTF-8.
pub fn dispatch(app: *mut NmpApp, namespace: &str, payload: serde_json::Value) -> serde_json::Value {
    let ns_c = CString::new(namespace).expect("namespace NUL-free");
    let payload_str = payload.to_string();
    let payload_c = CString::new(payload_str).expect("payload NUL-free");

    let result_ptr = nmp_app_dispatch_action(app, ns_c.as_ptr(), payload_c.as_ptr());

    if result_ptr.is_null() {
        return serde_json::Value::Null;
    }

    // SAFETY: `result_ptr` is a valid nul-terminated C string returned by
    // `nmp_app_dispatch_action`. We read it, copy the bytes, then free.
    let result_str = unsafe { CStr::from_ptr(result_ptr) }
        .to_str()
        .unwrap_or("{}")
        .to_owned();

    nmp_app_free_string(result_ptr);

    serde_json::from_str(&result_str).unwrap_or(serde_json::Value::Null)
}

/// Read the current podcast snapshot from the handle.
///
/// Returns `None` if the handle is null or the snapshot pointer is null.
pub fn snapshot(handle: *mut PodcastHandle) -> Option<PodcastUpdate> {
    let ptr = nmp_app_podcast_snapshot(handle);
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` is a valid nul-terminated C string returned by
    // `nmp_app_podcast_snapshot`. We read, copy, then free.
    let json = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("{}")
        .to_owned();
    nmp_app_podcast_snapshot_free(ptr);
    serde_json::from_str::<PodcastUpdate>(&json).ok()
}

/// Returns `true` if a TCP connection to `host:port` can be established within 2 seconds.
/// Used by scenarios to gate on optional external services (e.g. Ollama).
///
/// Resolves the hostname via DNS first (using `std::net::ToSocketAddrs`), then uses
/// `connect_timeout` on the first resolved address.
pub fn probe_tcp(host: &str, port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let timeout = Duration::from_secs(2);
    let addr_str = format!("{host}:{port}");
    match addr_str.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => TcpStream::connect_timeout(&addr, timeout).is_ok(),
            None => false,
        },
        Err(_) => false,
    }
}

/// Poll the snapshot every 100 ms until `pred` returns `true` or `timeout_ms`
/// elapses. Returns `Ok(update)` on success, `Err(msg)` on timeout.
///
/// Uses `nmp_app_podcast_snapshot_rev` (atomic read, no lock) to detect
/// when the store has changed, then reads the full snapshot. This avoids
/// blocking indefinitely on the store mutex while the actor thread is
/// doing a long-running subscribe write.
pub fn wait_for<F>(
    handle: *mut PodcastHandle,
    timeout_ms: u64,
    pred: F,
) -> Result<PodcastUpdate, String>
where
    F: Fn(&PodcastUpdate) -> bool,
{
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut last_rev = nmp_app_podcast_snapshot_rev(handle);
    loop {
        // Check deadline first so we don't do extra work past it.
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
        let rev = nmp_app_podcast_snapshot_rev(handle);
        if rev != last_rev {
            last_rev = rev;
            if let Some(update) = snapshot(handle) {
                if pred(&update) {
                    return Ok(update);
                }
            }
        }
    }
    Err(format!("wait_for timed out after {timeout_ms} ms"))
}

