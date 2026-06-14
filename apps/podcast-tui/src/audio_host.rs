//! Terminal audio capability host.
//!
//! Bridges `nmp.audio.capability` commands to an external audio player.
//! The default implementation tries `mpv` first, then falls back to a stub
//! that accepts commands so the kernel UI still works. The stub never
//! synthesizes a position — without a real backend the position is unknown
//! and is left unchanged rather than faked.
//!
//! ## mpv IPC
//!
//! When mpv is available we spawn it with `--input-ipc-server` and drive
//! playback through a Unix socket.  A sampler reads `playback-time` every
//! 250 ms in [`AudioHost::poll_position`].
//!
//! POSITION-SAMPLING EXCEPTION: libmpv / the mpv JSON IPC expose no per-frame
//! `playback-time` event, so periodic sampling is the only mechanism the
//! player offers — that 250 ms sample is a deliberate, documented exception
//! (see `docs/BACKLOG.md` `tui-mpv-position-sampling`), not a polling
//! shortcut.
//!
//! ## Kernel report wiring (D4/D7)
//!
//! `poll_position` enqueues an `AudioReport::Playing` after each successful
//! mpv position sample (≤4 Hz, matching the D8 ceiling).  Pause / Stop
//! transitions enqueue `AudioReport::Paused` / `AudioReport::Stopped`
//! immediately. The runtime drains these via [`AudioHost::drain_reports`] and
//! forwards them through `nmp_app_podcast_audio_report` — the same FFI seam
//! iOS and Android use — so the kernel projection reflects live TUI progress.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use nmp_app_podcast::{AudioCommand, AudioReport, AUDIO_CAPABILITY_NAMESPACE};
use serde::{Deserialize, Serialize};

/// Subprocess-based audio host.
pub struct AudioHost {
    mpv_child: Option<Child>,
    ipc_path: PathBuf,
    last_url: Option<String>,
    last_position_secs: f64,
    last_duration_secs: f64,
    is_playing: bool,
    mpv_available: bool,
    /// D4/D7 report queue. Enqueued by playback state transitions and
    /// position-sample ticks; drained by the runtime into
    /// `nmp_app_podcast_audio_report`.
    pending_reports: Vec<AudioReport>,
}

fn mpv_is_available() -> bool {
    Command::new("mpv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl AudioHost {
    pub fn new() -> Self {
        let ipc_path = std::env::temp_dir().join("podcast-tui-mpv.sock");
        Self {
            mpv_child: None,
            ipc_path,
            last_url: None,
            last_position_secs: 0.0,
            last_duration_secs: 0.0,
            is_playing: false,
            mpv_available: mpv_is_available(),
            pending_reports: Vec::new(),
        }
    }

    /// Drain all pending [`AudioReport`]s accumulated since the last call.
    ///
    /// The runtime passes each one to `nmp_app_podcast_audio_report` so the
    /// kernel projection stays in sync with live mpv playback (D4/D7).
    pub fn drain_reports(&mut self) -> Vec<AudioReport> {
        std::mem::take(&mut self.pending_reports)
    }

    pub fn handle_request(&mut self, request_str: &str) -> String {
        let req: CapabilityRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(e) => {
                return error_envelope("nmp.audio.capability", "", &format!("parse: {e}"));
            }
        };

        if req.namespace != AUDIO_CAPABILITY_NAMESPACE {
            return error_envelope(
                &req.namespace,
                &req.correlation_id,
                &format!("unexpected namespace: {}", req.namespace),
            );
        }

        let cmd: AudioCommand = match serde_json::from_str(&req.payload_json) {
            Ok(c) => c,
            Err(e) => {
                return error_envelope(
                    &req.namespace,
                    &req.correlation_id,
                    &format!("decode AudioCommand: {e}"),
                );
            }
        };

        let result_json = self.handle_audio_command(cmd);
        serde_json::to_string(&CapabilityEnvelope {
            namespace: req.namespace,
            correlation_id: req.correlation_id,
            result_json,
        })
        .unwrap_or_else(|_| "{}".to_string())
    }

    fn handle_audio_command(&mut self, cmd: AudioCommand) -> String {
        if !self.mpv_available {
            self.handle_stub_command(cmd)
        } else {
            self.handle_mpv_command(cmd)
        }
    }

    fn handle_mpv_command(&mut self, cmd: AudioCommand) -> String {
        match cmd {
            AudioCommand::Load {
                url, position_secs, ..
            } => {
                self.kill_mpv();
                self.last_url = Some(url.clone());
                self.last_position_secs = position_secs;
                self.last_duration_secs = 0.0;
                self.is_playing = true;

                let ipc = self.ipc_path.to_string_lossy().to_string();
                let child = match Command::new("mpv")
                    .arg("--no-video")
                    .arg("--force-window=never")
                    .arg(format!("--input-ipc-server={}", ipc))
                    .arg(format!("--start={}", position_secs))
                    .arg(&url)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(e) => {
                        return serde_json::json!({
                            "ok": false,
                            "error": format!("mpv spawn: {e}")
                        })
                        .to_string();
                    }
                };
                self.mpv_child = Some(child);
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::Play => {
                self.is_playing = true;
                let _ = self.mpv_ipc_set_property("pause", "false");
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::Pause => {
                self.is_playing = false;
                let _ = self.mpv_ipc_set_property("pause", "true");
                // D7: report the transition immediately so the kernel
                // projection reflects the paused state and flushes the
                // position to disk.
                if let Some(url) = self.last_url.clone() {
                    self.pending_reports.push(AudioReport::Paused {
                        url,
                        position_secs: self.last_position_secs,
                    });
                }
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::Seek { position_secs } => {
                self.last_position_secs = position_secs;
                let _ = self.mpv_ipc_command(&["seek", &format!("{}", position_secs), "absolute"]);
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::SetVolume { volume } => {
                let _ = self.mpv_ipc_set_property("volume", &format!("{}", volume * 100.0));
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::SetSpeed { speed } => {
                let _ = self.mpv_ipc_set_property("speed", &format!("{}", speed));
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::SetSleepTimer { secs } => {
                let _ = secs;
                serde_json::json!({"ok": true}).to_string()
            }
            AudioCommand::Stop => {
                self.kill_mpv();
                self.is_playing = false;
                // D7: report the stop so the kernel flushes the position
                // checkpoint to disk.
                self.pending_reports.push(AudioReport::Stopped);
                serde_json::json!({"ok": true}).to_string()
            }
        }
    }

    fn handle_stub_command(&mut self, cmd: AudioCommand) -> String {
        match cmd {
            AudioCommand::Load {
                url, position_secs, ..
            } => {
                self.last_url = Some(url);
                self.last_position_secs = position_secs;
                self.last_duration_secs = 0.0;
                self.is_playing = true;
            }
            AudioCommand::Play => {
                self.is_playing = true;
            }
            AudioCommand::Pause => {
                self.is_playing = false;
            }
            AudioCommand::Seek { position_secs } => {
                self.last_position_secs = position_secs;
            }
            AudioCommand::SetVolume { volume } => {
                let _ = volume;
            }
            AudioCommand::SetSpeed { speed } => {
                let _ = speed;
            }
            AudioCommand::SetSleepTimer { secs } => {
                let _ = secs;
            }
            AudioCommand::Stop => {
                self.is_playing = false;
            }
        }
        serde_json::json!({"ok": true}).to_string()
    }

    fn kill_mpv(&mut self) {
        if let Some(mut child) = self.mpv_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.ipc_path);
    }

    fn mpv_ipc_command(&self, args: &[&str]) -> std::io::Result<()> {
        let mut stream = UnixStream::connect(&self.ipc_path)?;
        let req = MpvRequest {
            command: args
                .iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
        };
        let line = serde_json::to_string(&req)? + "\n";
        stream.write_all(line.as_bytes())?;
        stream.flush()?;
        Ok(())
    }

    fn mpv_ipc_set_property(&self, name: &str, value: &str) -> std::io::Result<()> {
        let mut stream = UnixStream::connect(&self.ipc_path)?;
        let req = serde_json::json!({
            "command": ["set_property", name, value]
        });
        let line = req.to_string() + "\n";
        stream.write_all(line.as_bytes())?;
        stream.flush()?;
        Ok(())
    }

    /// Query mpv for a single numeric property, returning the value on success.
    fn mpv_ipc_get_number(&self, property: &str) -> Option<f64> {
        let mut stream = UnixStream::connect(&self.ipc_path).ok()?;
        let req = serde_json::json!({ "command": ["get_property", property] });
        let line = req.to_string() + "\n";
        stream.write_all(line.as_bytes()).ok()?;
        stream.flush().ok()?;

        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(resp) = serde_json::from_str::<MpvResponse>(&line) {
                    if let Some(data) = resp.data {
                        return serde_json::from_value::<f64>(data).ok();
                    }
                }
            }
            break;
        }
        None
    }

    /// Sample mpv's current `playback-time` and enqueue an
    /// `AudioReport::Playing` if playing.
    ///
    /// POSITION-SAMPLING EXCEPTION: mpv emits no position event, so this is the
    /// only way to observe `playback-time` (see the module docs and
    /// `docs/BACKLOG.md` `tui-mpv-position-sampling`).
    ///
    /// D8 cadence: called every 250 ms from the runtime tick loop (≤4 Hz).
    /// Only fires while mpv is running AND the host considers playback active.
    ///
    /// With no mpv backend there is no real position source. We do NOT
    /// synthesize progress here — the position is simply unknown and stays
    /// unchanged. (The old stub incremented `last_position_secs` by the tick
    /// interval, fabricating playback the player never produced; #322.)
    pub fn poll_position(&mut self) {
        if !self.mpv_available || self.mpv_child.is_none() || !self.is_playing {
            return;
        }

        // Sample position. If mpv is still starting up the IPC socket may not
        // yet exist; skip silently — the next 250 ms tick will retry.
        if let Some(pos) = self.mpv_ipc_get_number("playback-time") {
            self.last_position_secs = pos;

            // Sample duration; mpv returns 0/null before the stream header is
            // parsed, so keep the last valid value rather than zeroing it.
            if let Some(dur) = self.mpv_ipc_get_number("duration") {
                if dur > 0.0 {
                    self.last_duration_secs = dur;
                }
            }

            // D4/D7: forward position to the kernel projection via the same
            // AudioReport::Playing seam iOS and Android use (D8: ≤4 Hz).
            if let Some(url) = self.last_url.clone() {
                self.pending_reports.push(AudioReport::Playing {
                    url,
                    position_secs: self.last_position_secs,
                    duration_secs: self.last_duration_secs,
                });
            }
        }
    }
}

impl Drop for AudioHost {
    fn drop(&mut self) {
        self.kill_mpv();
    }
}

#[derive(Serialize)]
struct MpvRequest {
    command: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct MpvResponse {
    #[serde(default)]
    data: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct CapabilityRequest {
    namespace: String,
    correlation_id: String,
    payload_json: String,
}

#[derive(Serialize)]
struct CapabilityEnvelope {
    namespace: String,
    correlation_id: String,
    result_json: String,
}

fn error_envelope(namespace: &str, correlation_id: &str, msg: &str) -> String {
    let envelope = CapabilityEnvelope {
        namespace: namespace.to_owned(),
        correlation_id: correlation_id.to_owned(),
        result_json: serde_json::json!({"ok": false, "error": msg}).to_string(),
    };
    serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
}
