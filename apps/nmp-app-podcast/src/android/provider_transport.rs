//! Android JNI wrappers for shared provider transport FFI.

use std::ffi::{c_char, CStr, CString};

use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use nmp_ffi::nmp_app_free_string;

use super::session_ref;
use crate::ffi::{
    nmp_app_podcast_generate_image, nmp_app_podcast_provider_complete,
    nmp_app_podcast_provider_embed, nmp_app_podcast_rerank, PodcastHandle,
};

type PodcastJsonFn = extern "C" fn(*mut PodcastHandle, *const c_char) -> *mut c_char;

fn call_podcast_json_ffi<'l>(
    env: &mut JNIEnv<'l>,
    handle: jlong,
    request_json: JString<'l>,
    call: PodcastJsonFn,
) -> jstring {
    let null = std::ptr::null_mut();
    let Some(s) = session_ref(handle) else {
        return null;
    };
    if s.podcast.is_null() {
        return null;
    }
    let request = match env.get_string(&request_json) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return null,
    };
    let Ok(c_request) = CString::new(request) else {
        return null;
    };
    let result_ptr = call(s.podcast, c_request.as_ptr());
    if result_ptr.is_null() {
        return null;
    }
    let owned = unsafe { CStr::from_ptr(result_ptr) }
        .to_string_lossy()
        .into_owned();
    nmp_app_free_string(result_ptr);
    match env.new_string(owned) {
        Ok(js) => js.into_raw(),
        Err(_) => null,
    }
}

/// `nativeProviderComplete(handle, intentJson)` — shared provider completion.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeProviderComplete<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_provider_complete,
    )
}

/// `nativeProviderEmbed(handle, intentJson)` — shared provider embeddings.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeProviderEmbed<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_provider_embed,
    )
}

/// `nativeGenerateImage(handle, requestJson)` — shared image generation.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeGenerateImage<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    request_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        request_json,
        nmp_app_podcast_generate_image,
    )
}

/// `nativeRerank(handle, requestJson)` — shared RAG reranker.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeRerank<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    request_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(&mut env, handle, request_json, nmp_app_podcast_rerank)
}
