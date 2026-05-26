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
mod scenarios;

use std::ffi::CString;
use std::process::ExitCode;

use nmp_app_podcast::{
    nmp_app_podcast_register, nmp_app_podcast_set_data_dir, nmp_app_podcast_unregister,
};
use nmp_ffi::nmp_app_start;

fn main() -> ExitCode {
    // 1. Boot NmpApp.
    let app = harness::app_new();

    // 2. Install the headless capability host (real HTTP, no-op stubs for
    //    audio/download/notification).
    capability_host::install(app);

    // 3. Register Podcast projections and action modules.
    let handle = nmp_app_podcast_register(app);
    if handle.is_null() {
        eprintln!("[headless] nmp_app_podcast_register returned null");
        unsafe { harness::app_free(app) };
        return ExitCode::FAILURE;
    }

    // 4. Set a temp data dir so store serialisation has somewhere to write.
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path_cstr = CString::new(
        temp_dir.path().to_str().expect("temp dir is UTF-8")
    ).expect("path NUL-free");
    nmp_app_podcast_set_data_dir(handle, path_cstr.as_ptr());

    // 5. Start the kernel actor. Visible-limit and emit-hz are test defaults.
    //    _events_per_second is ignored by the kernel (see nmp_app_start in
    //    nmp-ffi/src/lib.rs).
    nmp_app_start(app, 0, 500, 10);

    // 7. Run all scenarios.
    let results = scenarios::run_all(app, handle);

    // 8. Print results.
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

    // 9. Tear down.
    nmp_app_podcast_unregister(handle);
    unsafe { harness::app_free(app) };

    if any_fail {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
