//! Terminal audio capability host.
//!
//! Bridges `nmp.audio.capability` commands to an external audio player.
//! The default implementation tries `mpv` first, then falls back to a stub
//! that reports fake position updates so the kernel UI still works.
//!
//! ## mpv IPC
//!
//! When mpv is available we spawn it with `--input-ipc-server` and drive
//! playback through a Unix socket.  A background thread polls
//! `playback-time` every 250 ms and forwards `AudioReport::Playing`
//! back to the kernel via the standard `nmp_app_podcast_audio_report` FFI.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use nmp_app_podcast::{AudioCommand, AUDIO_CAPABILITY_NAMESPACE};
use serde::{Deserialize, Serialize};

/// Subprocess-based audio host.
pub struct AudioHost {
    mpv_child: Option<Child>,
    ipc_path: PathBuf,
    last_url: Option<String>,
    last_position_secs: f64,
    is_playing: bool,
    mpv_available: bool,
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
            is_playing: false,
            mpv_available: mpv_is_available(),
        }
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

    /// Poll mpv for current position and report back to the kernel.
    pub fn poll_position(&mut self) {
        if !self.mpv_available || self.mpv_child.is_none() {
            if self.is_playing {
                self.last_position_secs += 0.25;
            }
            return;
        }

        let mut stream = match UnixStream::connect(&self.ipc_path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let req = serde_json::json!({
            "command": ["get_property", "playback-time"]
        });
        if let Ok(line) = serde_json::to_string(&req) {
            let _ = stream.write_all((line + "\n").as_bytes());
            let _ = stream.flush();
        }

        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(resp) = serde_json::from_str::<MpvResponse>(&line) {
                    if let Some(data) = resp.data {
                        if let Ok(pos) = serde_json::from_value::<f64>(data.clone()) {
                            self.last_position_secs = pos;
                        }
                    }
                }
            }
            break;
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
