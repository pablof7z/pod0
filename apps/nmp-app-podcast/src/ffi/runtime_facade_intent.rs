use std::ffi::c_char;

use nmp_core::substrate::{
    InputIntentClassification, InputIntentRejection, InputIntentRequest,
};
use nmp_native_runtime::NmpApp;
use serde::Serialize;

use super::{app_ref, c_string, into_c_string, optional_c_string};

#[derive(Serialize)]
struct FfiError {
    ok: bool,
    error: &'static str,
}

#[derive(Serialize)]
struct ClassifyOk<'a> {
    ok: bool,
    classification: &'a InputIntentClassification,
}

fn error_json(error: &'static str) -> String {
    serde_json::to_string(&FfiError { ok: false, error })
        .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization-failed"}"#.to_string())
}

fn classify_request(
    app: *mut NmpApp,
    request_json: *const c_char,
) -> Result<InputIntentClassification, &'static str> {
    let app = app_ref(app).ok_or("invalid-app")?;
    let request_json = c_string(request_json).ok_or("invalid-input")?;
    let request: InputIntentRequest =
        serde_json::from_str(&request_json).map_err(|_| "unparseable-request")?;
    Ok(app.classify_input_intent(&request))
}

#[no_mangle]
pub extern "C" fn nmp_app_intent_classify(
    app: *mut NmpApp,
    request_json: *const c_char,
) -> *mut c_char {
    let output = match classify_request(app, request_json) {
        Ok(classification) => serde_json::to_string(&ClassifyOk {
            ok: true,
            classification: &classification,
        })
        .unwrap_or_else(|_| error_json("serialization-failed")),
        Err(error) => error_json(error),
    };
    into_c_string(output)
}

#[derive(Serialize)]
struct Dispatched<'a> {
    ok: bool,
    dispatched: &'a nmp_core::substrate::InputIntentCandidate,
}

#[derive(Serialize)]
struct Rejected<'a> {
    ok: bool,
    rejection: &'a InputIntentRejection,
}

#[no_mangle]
pub extern "C" fn nmp_app_intent_dispatch(
    app: *mut NmpApp,
    request_json: *const c_char,
    session_id: *const c_char,
) -> *mut c_char {
    let output = match dispatch_intent(app, request_json, session_id) {
        Ok(output) => output,
        Err(error) => error_json(error),
    };
    into_c_string(output)
}

fn dispatch_intent(
    app: *mut NmpApp,
    request_json: *const c_char,
    session_id: *const c_char,
) -> Result<String, &'static str> {
    let app_ref = app_ref(app).ok_or("invalid-app")?;
    let request_json = c_string(request_json).ok_or("invalid-input")?;
    let request: InputIntentRequest =
        serde_json::from_str(&request_json).map_err(|_| "unparseable-request")?;
    match app_ref.dispatch_input_intent(&request, optional_c_string(session_id).as_deref()) {
        nmp_native_runtime::InputIntentDispatch::Dispatched(candidate) => {
            serde_json::to_string(&Dispatched {
                ok: true,
                dispatched: &candidate,
            })
            .map_err(|_| "serialization-failed")
        }
        nmp_native_runtime::InputIntentDispatch::Rejection(rejection) => {
            serde_json::to_string(&Rejected {
                ok: true,
                rejection: &rejection,
            })
            .map_err(|_| "serialization-failed")
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
enum DecodeTarget {
    Profile {
        pubkey: String,
        relays: Vec<String>,
    },
    Event {
        event_id: String,
        relays: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<u32>,
    },
    Address {
        identifier: String,
        pubkey: String,
        kind: u32,
        relays: Vec<String>,
    },
}

#[derive(Serialize)]
struct DecodeSuccess {
    ok: bool,
    #[serde(flatten)]
    target: DecodeTarget,
}

#[no_mangle]
pub extern "C" fn nmp_nip21_decode_uri(input: *const c_char) -> *mut c_char {
    let output = c_string(input)
        .map(|input| decode_uri_json(&input))
        .unwrap_or_else(|| error_json("invalid-input"));
    into_c_string(output)
}

fn decode_uri_json(input: &str) -> String {
    match decode_uri(input) {
        Ok(target) => serde_json::to_string(&DecodeSuccess { ok: true, target })
            .unwrap_or_else(|_| error_json("serialization-failed")),
        Err(error) => error_json(error),
    }
}

fn decode_uri(input: &str) -> Result<DecodeTarget, &'static str> {
    let target = if input.starts_with("nostr:") {
        nmp_nostr_id::parse_nostr_uri(input).map_err(|_| "unparseable")?
    } else {
        bare_entity_to_target(nmp_nostr_id::parse(input).map_err(|_| "unparseable")?)?
    };
    Ok(match target {
        nmp_nostr_id::NostrUri::Profile { pubkey, relays } => {
            DecodeTarget::Profile { pubkey, relays }
        }
        nmp_nostr_id::NostrUri::Event {
            event_id,
            relays,
            author,
            kind,
        } => DecodeTarget::Event {
            event_id,
            relays,
            author,
            kind,
        },
        nmp_nostr_id::NostrUri::Address {
            identifier,
            pubkey,
            kind,
            relays,
        } => DecodeTarget::Address {
            identifier,
            pubkey,
            kind,
            relays,
        },
    })
}

fn bare_entity_to_target(
    entity: nmp_nostr_id::Nip19Entity,
) -> Result<nmp_nostr_id::NostrUri, &'static str> {
    use nmp_nostr_id::Nip19Entity::{Naddr, Nevent, Note, Nprofile, Npub, Nsec};
    match entity {
        Nsec(_) => Err("nsec-forbidden"),
        Npub(pubkey) => Ok(nmp_nostr_id::NostrUri::Profile {
            pubkey,
            relays: Vec::new(),
        }),
        Nprofile(data) => Ok(nmp_nostr_id::NostrUri::Profile {
            pubkey: data.pubkey,
            relays: data.relays,
        }),
        Note(event_id) => Ok(nmp_nostr_id::NostrUri::Event {
            event_id,
            relays: Vec::new(),
            author: None,
            kind: None,
        }),
        Nevent(data) => Ok(nmp_nostr_id::NostrUri::Event {
            event_id: data.event_id,
            relays: data.relays,
            author: data.author,
            kind: data.kind,
        }),
        Naddr(data) => Ok(nmp_nostr_id::NostrUri::Address {
            identifier: data.identifier,
            pubkey: data.pubkey,
            kind: data.kind,
            relays: data.relays,
        }),
    }
}
