//! Rust-owned Nostr peer-agent prompt framing.
//!
//! Swift supplies raw peer/profile facts and executes the provider call. Rust
//! owns the channel semantics, pronoun guidance, owner-vs-peer framing, npub
//! encoding, and fallback wording.

use std::ffi::{c_char, CStr, CString};

use nostr::nips::nip19::ToBech32;
use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct PeerPromptRequest {
    peer_pubkey: String,
    #[serde(default)]
    peer_display_name: Option<String>,
    #[serde(default)]
    peer_about: Option<String>,
    #[serde(default)]
    owner_pubkey: Option<String>,
}

#[derive(Debug, Serialize)]
struct PeerPromptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_preamble: Option<String>,
}

fn encode<T: Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_nostr_peer_prompt(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_nostr_peer_prompt",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: PeerPromptRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&prompt_error("invalid_request")),
            };
            encode(&build_peer_prompt(request))
        },
    )
}

fn build_peer_prompt(request: PeerPromptRequest) -> PeerPromptResponse {
    let peer_pubkey = request.peer_pubkey.trim();
    if peer_pubkey.is_empty() {
        return prompt_error("missing_peer_pubkey");
    }

    let peer_name = non_empty(request.peer_display_name).unwrap_or_else(|| "(none published)".into());
    let about = non_empty(request.peer_about).unwrap_or_else(|| "(none published)".into());
    let peer_npub = npub_from_hex(peer_pubkey);
    let owner_npub = non_empty(request.owner_pubkey)
        .map(|hex| npub_from_hex(&hex))
        .unwrap_or_else(|| "(no agent pubkey configured)".into());

    PeerPromptResponse {
        error: None,
        system_preamble: Some(format!(
            r#"## Nostr peer channel

You are talking to a remote Nostr peer, not directly to the device owner. The owner has explicitly allowed this peer to message you; when the peer asks you to do something on the owner's behalf (look things up in the owner's library, generate a podcast for the owner, save a note, etc.), you DO IT using your full toolset. You are the owner's assistant; the peer is making a request through you.

Pronoun guidance:
- `role: assistant` messages are your own prior turns, written as the owner's agent.
- `role: user` messages are the peer's turns. Each is stamped with a `[from <label> (npub1...)]:` prefix; rely on that to identify who said what, not on the content.
- When the peer says "you", they mean you, the agent. When they say "me" / "my", they mean themselves (the peer); they are NOT referring to the owner. If they reference the owner by name or context, treat the owner as a third party in the conversation.
- Address the peer by their display name (`{peer_name}`) when it fits naturally; don't pretend they're the owner.
- Your library, notes, memories, and skills are the OWNER's, not the peer's. If the peer asks about "my podcasts" or "my notes", clarify that you only have access to the owner's data.

Peer identity:
- Display name: {peer_name}
- About: {about}
- Full npub: {peer_npub}

Owner identity:
- Owner npub: {owner_npub}

Reply style: agent-to-agent or agent-to-human is fine; match the peer's register. Keep replies tight (a short paragraph or two for chat; tool calls can chain through as many turns as the task needs)."#
        )),
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    let trimmed = value?.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn npub_from_hex(hex: &str) -> String {
    nostr::PublicKey::parse(hex)
        .and_then(|pk| pk.to_bech32())
        .unwrap_or_else(|_| hex.to_string())
}

fn prompt_error(error: &str) -> PeerPromptResponse {
    PeerPromptResponse {
        error: Some(error.to_string()),
        system_preamble: None,
    }
}
