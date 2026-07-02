//! Scenario: download lifecycle — materialize, pause, resume, cancel.
//!
//! ## Why this scenario exists
//!
//! The download projection is the most crash-prone in the tree:
//!
//! * **#442 dup-key crash** — dispatching `download` for an episode already in
//!   the queue (or one whose stale terminal record was not cleared) caused a
//!   fatal key-collision in the snapshot builder. Guarded here by dispatching
//!   the same `episode_id` twice and asserting no panic occurs and the
//!   projection returns exactly one row (idempotence).
//!
//! * **#463 restored-downloads-vanishing** — after a completed download was
//!   "restored" (re-enqueued via a cold-start path), the snapshot builder
//!   dropped the row because the terminal `Completed` state was filtered out
//!   before re-enqueuing had a chance to transition it back to `Active`. Guarded
//!   here by verifying a re-dispatched download for a previously-enqueued
//!   episode re-materializes cleanly (the idempotence re-enqueue path).
//!
//! ## Network-free assertion strategy
//!
//! All projection-state assertions are **network-free**: they exercise the
//! kernel's in-memory `DownloadQueue` and `bump_domain(Downloads)` path — not
//! actual HTTP byte-fetching. The executor capability is a no-op stub in the
//! headless binary (see `capability_host.rs`), so `StartDownload` commands are
//! issued but never acted on. The queue item remains in `Active` state
//! indefinitely, which is exactly what we need to test the projection shape.
//!
//! Network-dependent assertions (e.g. verifying bytes actually land on disk)
//! are out of scope here and would be Skipped — but they don't arise because
//! the scenario asserts kernel-state rows only.
//!
//! ## Seam coverage
//!
//! ```text
//! dispatch("podcast.player", {"op":"download","episode_id":"...","url":"..."})
//!   → PlayerActionModule::execute
//!   → PodcastHostOpHandler::handle_player_action(Download { episode_id, url })
//!   → handle_player_download
//!   → start_episode_download (canonical single path)
//!   → DownloadQueue::enqueue         ← idempotence / dup-key guard (#442)
//!   → bump_domain(Downloads)         ← rev bump drives wait_for
//!   → snapshot: PodcastUpdate.downloads materializes  ← row visibility (#463)
//! ```
//!
//! pause / resume / cancel route through `handle_download_command`, which
//! calls `DownloadQueue::pause` / `resume` / `cancel` and bumps the domain.
//! State transitions are asserted via the snapshot after each dispatch.
//!
//! ## Prerequisite: library episode
//!
//! The `handle_player_download` handler validates the episode exists in the
//! store before enqueuing. We seed it by re-using the mock-feed subscribe
//! path (same as `rss_subscribe`). If loopback TCP is unavailable the whole
//! scenario Skips — the library seeding is genuinely network-dependent
//! (loopback is "network" here: bind + connect on 127.0.0.1).

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_native_runtime::NmpApp;
use serde_json::json;

use crate::harness::{dispatch, snapshot, wait_for};
use crate::mock_feed;
use crate::scenarios::ScenarioResult::{self, Fail, Pass, Skip};

/// Namespace for `podcast.player.*` actions.
const PLAYER_NS: &str = "podcast.player";

/// Verify that loopback TCP is usable. Same probe used by `rss_subscribe`.
fn probe_loopback() -> bool {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return false;
    };
    let port = match listener.local_addr() {
        Ok(a) => a.port(),
        Err(_) => return false,
    };
    let addr: SocketAddr = match format!("127.0.0.1:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok()
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // ── Prerequisite: loopback TCP ────────────────────────────────────────────
    //
    // We need loopback to serve the mock RSS feed so the store has a real
    // episode to validate against. If loopback is unavailable, Skip.
    if !probe_loopback() {
        return Skip("loopback TCP unavailable; cannot seed library episode".into());
    }

    // ── Step 1: seed library via mock RSS feed ────────────────────────────────
    //
    // Reuse the same mock feed approach as `rss_subscribe`. We start a new
    // mock server regardless of whether a prior scenario already subscribed —
    // the Subscribe action is idempotent (dedups by feed URL) but we need
    // a fresh server to serve the response. If the library is already
    // populated from a prior scenario, `wait_for` will satisfy immediately.
    let port = mock_feed::start();
    let feed_url = format!("http://127.0.0.1:{port}/feed.xml");

    let sub_res = dispatch(
        app,
        "podcast",
        json!({"op": "subscribe", "feed_url": feed_url}),
    );
    if let Some(err) = sub_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("subscribe dispatch rejected: {err}"));
    }

    let seeded = match wait_for(handle, 10_000, |u| {
        !u.library.is_empty() && !u.library[0].episodes.is_empty()
    }) {
        Ok(u) => u,
        Err(msg) => return Fail(format!("library never seeded: {msg}")),
    };

    // Pick the first episode as our download target.
    let episode = &seeded.library[0].episodes[0];
    let episode_id = episode.id.clone();
    // The mock feed enclosure URL — the headless capability host stubs out
    // the actual HTTP fetch, so this URL is never fetched; it only needs to
    // be non-empty and match what the store has on file for episode_id.
    let enclosure_url = episode
        .enclosure_url
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1/ep1.mp3".to_string());

    // ── Step 2: dispatch Download and assert row materializes ─────────────────
    //
    // This exercises the primary projection path. `handle_player_download`
    // validates the episode in the store, enqueues via `DownloadQueue::enqueue`,
    // and calls `bump_domain(Downloads)`. The next snapshot frame must carry
    // `downloads.active` with a row for our episode.
    //
    // Mutation-sanity: if the projection is broken (e.g. the bump_domain call
    // is missing), `wait_for` times out → Fail. If the row shape is wrong
    // (missing episode_id), the assertion below → Fail.
    let dl_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "download", "episode_id": episode_id, "url": enclosure_url}),
    );
    if let Some(err) = dl_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("download dispatch rejected: {err}"));
    }

    let after_dl = match wait_for(handle, 5_000, |u| {
        u.downloads
            .as_ref()
            .is_some_and(|dq| dq.active.iter().any(|item| item.episode_id == episode_id))
    }) {
        Ok(u) => u,
        Err(msg) => return Fail(format!(
            "download row never materialized in projection: {msg}"
        )),
    };

    let dq = after_dl.downloads.as_ref().unwrap();
    let row = dq
        .active
        .iter()
        .find(|r| r.episode_id == episode_id)
        .unwrap();

    // Row must be active or queued (the executor is a stub so it will be
    // active because the concurrency cap allows it).
    if row.state != "active" && row.state != "queued" {
        return Fail(format!(
            "expected download row state 'active' or 'queued', got '{}'",
            row.state
        ));
    }
    if row.progress < 0.0 || row.progress > 1.0 {
        return Fail(format!(
            "progress out of range 0.0..=1.0: {}",
            row.progress
        ));
    }

    // ── Step 3: dup-key guard (#442) ──────────────────────────────────────────
    //
    // Dispatch download for the SAME episode_id a second time. The
    // `DownloadQueue::enqueue` idempotence guard must make this a silent
    // no-op (the item is still Active → non-terminal → early return).
    // The critical assertion: NO PANIC and exactly ONE row for episode_id.
    let dup_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "download", "episode_id": episode_id, "url": enclosure_url}),
    );
    // Must not carry a synchronous error (the action itself is valid).
    if let Some(err) = dup_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!(
            "duplicate download dispatch returned error: {err}"
        ));
    }

    // Give the kernel one tick to process the second dispatch.
    std::thread::sleep(Duration::from_millis(200));

    let after_dup = match snapshot(handle) {
        Some(u) => u,
        None => return Fail("snapshot returned None after dup dispatch".into()),
    };

    let dup_count = after_dup
        .downloads
        .as_ref()
        .map(|dq| {
            dq.active
                .iter()
                .filter(|r| r.episode_id == episode_id)
                .count()
        })
        .unwrap_or(0);

    if dup_count != 1 {
        return Fail(format!(
            "expected exactly 1 download row for episode_id after dup dispatch, got {dup_count} \
             (dup-key crash class #442)"
        ));
    }

    // ── Step 4: pause transition ──────────────────────────────────────────────
    //
    // Dispatch PauseDownload. The `DownloadQueue::pause` method returns a
    // `PauseDownload` command when the item is Active and bumps the domain.
    // We verify the row moves to "paused" in the snapshot.
    //
    // Note: in the headless binary the executor stub doesn't send back a
    // `Paused` report, so the item state is NOT changed by handle_report.
    // Instead, the pause command is dispatched to the no-op capability and
    // the state remains `Active` at the queue layer.
    // We assert the dispatch is accepted (no synchronous error) and the
    // row is still visible — which directly guards that pause dispatch
    // does NOT remove the row from the projection (#463 visibility class).
    let pause_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "pause_download", "episode_id": episode_id}),
    );
    if let Some(err) = pause_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("pause_download dispatch rejected: {err}"));
    }

    // Small sleep to let the actor thread process the command.
    std::thread::sleep(Duration::from_millis(200));

    let after_pause = match snapshot(handle) {
        Some(u) => u,
        None => return Fail("snapshot returned None after pause_download".into()),
    };

    let pause_row_visible = after_pause
        .downloads
        .as_ref()
        .is_some_and(|dq| dq.active.iter().any(|r| r.episode_id == episode_id));
    if !pause_row_visible {
        return Fail(
            "download row vanished from projection after pause_download \
             (restored-visibility class #463)"
                .into(),
        );
    }

    // ── Step 5: resume transition ─────────────────────────────────────────────
    //
    // Dispatch ResumeDownload. Accepted without error → row still visible.
    let resume_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "resume_download", "episode_id": episode_id}),
    );
    if let Some(err) = resume_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("resume_download dispatch rejected: {err}"));
    }

    std::thread::sleep(Duration::from_millis(200));

    let after_resume = match snapshot(handle) {
        Some(u) => u,
        None => return Fail("snapshot returned None after resume_download".into()),
    };

    let resume_row_visible = after_resume
        .downloads
        .as_ref()
        .is_some_and(|dq| dq.active.iter().any(|r| r.episode_id == episode_id));
    if !resume_row_visible {
        return Fail(
            "download row vanished from projection after resume_download \
             (restored-visibility class #463)"
                .into(),
        );
    }

    // ── Step 6: cancel transition ─────────────────────────────────────────────
    //
    // Dispatch CancelDownload. A queued item is cancelled synchronously;
    // an active item gets a CancelDownload command dispatched to the (stub)
    // executor and a `Cancelled` report would normally follow. In the
    // headless binary the executor is a no-op stub so the `Cancelled`
    // report never arrives — the item stays in the map but the cancel
    // command is accepted. We assert:
    //   * No synchronous error from the dispatch.
    //   * The cancel command was dispatched successfully by the kernel
    //     (no error in dispatch return).
    //
    // We intentionally do NOT assert the row disappears from the snapshot
    // here, because the headless executor never sends back a `Cancelled`
    // report that would drive the state transition. That is the correct
    // offline boundary: row removal is driven by the executor report path,
    // not by the cancel dispatch itself.
    //
    // The assertion that matters is: cancel dispatch is accepted without
    // panic and without error, proving the download-cancel action seam is
    // intact.
    let cancel_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "cancel_download", "episode_id": episode_id}),
    );
    if let Some(err) = cancel_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("cancel_download dispatch rejected: {err}"));
    }

    // ── Step 7: cancel_all_downloads ─────────────────────────────────────────
    //
    // Dispatch CancelAllDownloads. Accepted without error.
    // Queued items are cancelled synchronously; active items get the
    // stub executor CancelAll command. No panic = #442 regression clear.
    let cancel_all_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "cancel_all_downloads"}),
    );
    if let Some(err) = cancel_all_res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("cancel_all_downloads dispatch rejected: {err}"));
    }

    // ── Step 8: restored-download visibility guard (#463) ────────────────────
    //
    // Simulate a "restored download": re-dispatch the same episode_id that
    // was previously Active (and for which cancel was issued but the report
    // has not arrived because the executor is a stub). The
    // `DownloadQueue::enqueue` idempotence check sees the item's existing
    // state; if it is not terminal (Active/Paused post-cancel-command-with-
    // no-report), it is a no-op. If it IS terminal (Cancelled, because the
    // cancel command was to a Queued item that was cancelled synchronously),
    // the stale record is wiped and a fresh Active record is created.
    //
    // Either path must produce a visible row in the projection — this is the
    // core of the #463 regression: rows must NOT vanish after re-enqueue.
    let restore_res = dispatch(
        app,
        PLAYER_NS,
        json!({"op": "download", "episode_id": episode_id, "url": enclosure_url}),
    );
    if let Some(err) = restore_res.get("error").and_then(|v| v.as_str()) {
        // The handler validates that the episode exists in the store.
        // If it returns an error here something went wrong with the store
        // rather than the restore path.
        return Fail(format!(
            "restore download dispatch rejected (expected acceptance): {err}"
        ));
    }

    // Wait for the projection to reflect any post-restore row. Two outcomes
    // are both acceptable and not regressions:
    //
    // a) The item was already in a non-terminal state (Active, because the
    //    stub executor never sent `Cancelled`) → enqueue is a no-op, the
    //    existing row stays visible → `downloads.active` is non-empty.
    //
    // b) The item was Cancelled synchronously (e.g. it was Queued at cancel
    //    time) → enqueue creates a fresh Active row → `downloads.active`
    //    gets the row after the domain bump.
    //
    // Regression outcome (what #463 looked like): the row is absent even
    // though a download was enqueued — the projection filtered it out.
    // `wait_for` detects this as a timeout → Fail.
    match wait_for(handle, 3_000, |u| {
        u.downloads
            .as_ref()
            .is_some_and(|dq| !dq.active.is_empty())
    }) {
        Ok(_) => {}
        Err(_) => {
            // The row may genuinely be absent if the prior cancel was to a
            // non-terminal item AND the re-enqueue was a no-op AND somehow
            // the snapshot was emitted before the domain bump landed. Read
            // the current state directly for a final check.
            let current = snapshot(handle);
            let still_empty = current
                .as_ref()
                .and_then(|u| u.downloads.as_ref())
                .map(|dq| dq.active.is_empty())
                .unwrap_or(true);
            if still_empty {
                return Fail(
                    "restored-download row absent from projection (regression class #463): \
                     downloads.active is empty after re-enqueue"
                        .into(),
                );
            }
        }
    }

    Pass
}
