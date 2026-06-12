use std::ffi::{CStr, CString};

use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;
use nmp_ffi::nmp_app_free_string;

use crate::ffi::{
    nmp_app_podcast_audio_report, nmp_app_podcast_download_report, nmp_app_podcast_http_report,
};
use crate::ffi::guard::ffi_guard;

/// `nativeCapabilityReport(handle, namespace, reportJson)` — handle-aware
/// host → kernel report channel. Audio reports project into the Rust
/// `PlayerActor` and may return a follow-up `AudioCommand` JSON.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeCapabilityReport<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    namespace: JString<'l>,
    report_json: JString<'l>,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard("nativeCapabilityReport", || null, || {
        let Some(s) = super::session_ref(handle) else {
            return null;
        };
        if s.podcast.is_null() {
            return null;
        }
        let ns = match env.get_string(&namespace) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return null,
        };
        let body = match env.get_string(&report_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return null,
        };
        if ns != crate::capability::AUDIO_CAPABILITY_NAMESPACE {
            return null;
        }
        let Ok(c_body) = CString::new(body) else {
            return null;
        };
        let follow_up_ptr = nmp_app_podcast_audio_report(s.podcast, c_body.as_ptr());
        response_string(&mut env, follow_up_ptr)
    })
}

/// `nativeDownloadReport(handle, reportJson)` — handle-aware download report
/// channel. Android starts downloads from projected `downloads.active` rows so
/// there is one starter/canceller and no duplicate path competing with the
/// Rust queue.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeDownloadReport<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    report_json: JString<'l>,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard("nativeDownloadReport", || null, || {
        let Some(s) = super::session_ref(handle) else {
            return null;
        };
        if s.podcast.is_null() {
            return null;
        }
        let body = match env.get_string(&report_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return null,
        };
        let Ok(c_body) = CString::new(body) else {
            return null;
        };
        let follow_up_ptr = nmp_app_podcast_download_report(s.podcast, c_body.as_ptr());
        response_string(&mut env, follow_up_ptr)
    })
}

/// `nativeHttpReport(handle, reportJson)` — handle-aware async HTTP report
/// channel for the optimistic-subscribe feed fetch. The Android executor runs
/// the RSS fetch off the actor thread (mirroring `nativeDownloadReport`) and
/// posts the JSON-encoded `HttpReport` here; the kernel resolves the matching
/// pending fetch and bumps the snapshot rev.
///
/// Unlike downloads there is no follow-up command — `nmp_app_podcast_http_report`
/// always returns NULL — so this entry point is `void` (the Kotlin declaration
/// is `external fun nativeHttpReport(...): Unit`). Any non-null return is freed
/// defensively. D6: every failure path returns silently, never panics.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeHttpReport<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    report_json: JString<'l>,
) {
    ffi_guard("nativeHttpReport", || (), || {
        let Some(s) = super::session_ref(handle) else {
            return;
        };
        if s.podcast.is_null() {
            return;
        }
        let body = match env.get_string(&report_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return,
        };
        let Ok(c_body) = CString::new(body) else {
            return;
        };
        let ret = nmp_app_podcast_http_report(s.podcast, c_body.as_ptr());
        if !ret.is_null() {
            nmp_app_free_string(ret);
        }
    });
}

fn response_string(env: &mut JNIEnv<'_>, ptr: *mut std::ffi::c_char) -> jstring {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let owned = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    nmp_app_free_string(ptr);
    match env.new_string(owned) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
