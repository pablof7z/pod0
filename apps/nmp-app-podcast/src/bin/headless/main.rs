//! Headless scenario test binary for `nmp-app-podcast`.
//!
//! Boots the full NMP + Podcast kernel via the C-ABI, installs a real HTTP
//! capability host (reqwest), runs scripted scenarios, and exits 0 (all pass
//! or skip) or 1 (any failure).
//!
//! Usage:
//!   cargo run -p nmp-app-podcast --bin headless --features headless

mod capability_host;
mod fixtures;
mod harness;
mod mock_feed;
mod relay_client;
mod scenarios;
mod sign_tap;

use std::ffi::CString;
use std::process::ExitCode;

use nmp_app_podcast::{
    nmp_app_podcast_register, nmp_app_podcast_set_data_dir, nmp_app_podcast_unregister,
};

fn main() -> ExitCode {
    // 1. Boot NmpApp.
    let app = harness::app_new();

    // 2. Install the headless capability host (real HTTP, no-op stubs for
    //    audio/download/notification).
    capability_host::install(app);

    // 2b. Install the signed-event capture tap (update-frame callback) BEFORE
    //     start, so the nipf4_publish scenario can observe the kernel's signed
    //     output via the `signed_events` push projection. See sign_tap.rs.
    sign_tap::install(app);

    // 3. Register Podcast projections and action modules.
    let handle = nmp_app_podcast_register(app);
    if handle.is_null() {
        unsafe { harness::app_free(app) };
        return ExitCode::FAILURE;
    }

    // 3b. Wire the handle into the capability host so the async HTTP path
    //     can call nmp_app_podcast_http_report after completing a feed fetch.
    //     This must happen after register (handle is now valid) and before
    //     start (capability calls can arrive from the very first kernel tick).
    capability_host::set_handle(handle);

    // 4. Set a temp data dir so store serialisation has somewhere to write.
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path_cstr =
        CString::new(temp_dir.path().to_str().expect("temp dir is UTF-8")).expect("path NUL-free");
    nmp_app_podcast_set_data_dir(handle, path_cstr.as_ptr());

    // 5. Declare the explicit all-builtins projection intent, then start the
    //    kernel actor. Visible-limit and emit-hz are test defaults.
    if !app.is_null() {
        // SAFETY: app is allocated by harness::app_new and remains live until
        // harness::app_free at teardown.
        let app_ref = unsafe { &*app };
        app_ref.consume_all_builtin_projections();
        app_ref.start_runtime(500, 10);
    }

    // 6. Run all scenarios.
    let results = scenarios::run_all(app, handle);

    // 7. Print results.
    let mut any_fail = false;
    for (name, result) in &results {
        let label = result.label();
        match result {
            scenarios::ScenarioResult::Pass => {
                println!("{label:<6} {name}");
            }
            scenarios::ScenarioResult::Skip(reason) => {
                println!("{label:<6} {name} ({reason})");
            }
            scenarios::ScenarioResult::Fail(reason) => {
                println!("{label:<6} {name}: {reason}");
                any_fail = true;
            }
        }
    }

    // 8. Tear down.
    nmp_app_podcast_unregister(handle);
    unsafe { harness::app_free(app) };

    if any_fail {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
