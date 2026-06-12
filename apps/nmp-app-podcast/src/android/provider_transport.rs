//! Android JNI wrappers for shared provider transport FFI.

use std::ffi::{c_char, CStr, CString};

use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use nmp_ffi::nmp_free_string;

use super::session_ref;
use crate::ffi::guard::ffi_guard;
use crate::ffi::{
    nmp_app_podcast_assemblyai_transcribe, nmp_app_podcast_byok_authorization,
    nmp_app_podcast_byok_exchange, nmp_app_podcast_chat_complete,
    nmp_app_podcast_elevenlabs_scribe_transcribe, nmp_app_podcast_elevenlabs_tts_synthesize,
    nmp_app_podcast_elevenlabs_voice_catalog, nmp_app_podcast_generate_image,
    nmp_app_podcast_local_model_catalog, nmp_app_podcast_openrouter_whisper_transcribe,
    nmp_app_podcast_perplexity_search, nmp_app_podcast_provider_complete,
    nmp_app_podcast_provider_embed, nmp_app_podcast_provider_model_catalog, nmp_app_podcast_rerank,
    nmp_app_podcast_speech_model_catalog, nmp_app_podcast_validate_elevenlabs_key,
    nmp_app_podcast_validate_openrouter_key, PodcastHandle,
};

type PodcastJsonFn = extern "C" fn(*mut PodcastHandle, *const c_char) -> *mut c_char;
type PodcastCatalogFn = extern "C" fn(*mut PodcastHandle) -> *mut c_char;
type PodcastGlobalJsonFn = extern "C" fn(*const c_char) -> *mut c_char;

fn java_string<'l>(env: &JNIEnv<'l>, value: String) -> jstring {
    match env.new_string(value) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn call_podcast_json_ffi<'l>(
    env: &mut JNIEnv<'l>,
    handle: jlong,
    request_json: JString<'l>,
    call: PodcastJsonFn,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard("call_podcast_json_ffi", || null, || {
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
        nmp_free_string(result_ptr);
        java_string(env, owned)
    })
}

fn call_podcast_global_json_ffi<'l>(
    env: &mut JNIEnv<'l>,
    request_json: JString<'l>,
    call: PodcastGlobalJsonFn,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard("call_podcast_global_json_ffi", || null, || {
        let request = match env.get_string(&request_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return null,
        };
        let Ok(c_request) = CString::new(request) else {
            return null;
        };
        let result_ptr = call(c_request.as_ptr());
        if result_ptr.is_null() {
            return null;
        }
        let owned = unsafe { CStr::from_ptr(result_ptr) }
            .to_string_lossy()
            .into_owned();
        nmp_free_string(result_ptr);
        java_string(env, owned)
    })
}

fn call_podcast_catalog_ffi<'l>(
    env: &JNIEnv<'l>,
    handle: jlong,
    call: PodcastCatalogFn,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard("call_podcast_catalog_ffi", || null, || {
        let Some(s) = session_ref(handle) else {
            return null;
        };
        if s.podcast.is_null() {
            return null;
        }
        let result_ptr = call(s.podcast);
        if result_ptr.is_null() {
            return null;
        }
        let owned = unsafe { CStr::from_ptr(result_ptr) }
            .to_string_lossy()
            .into_owned();
        nmp_free_string(result_ptr);
        java_string(env, owned)
    })
}

/// `nativeByokAuthorization(intentJson)` - shared BYOK authorization URL.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeByokAuthorization<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_global_json_ffi(&mut env, intent_json, nmp_app_podcast_byok_authorization)
}

/// `nativeByokExchange(handle, intentJson)` - shared BYOK token exchange.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeByokExchange<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(&mut env, handle, intent_json, nmp_app_podcast_byok_exchange)
}

/// `nativeChatComplete(handle, messagesJson)` — shared agent chat completion.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeChatComplete<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    messages_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        messages_json,
        nmp_app_podcast_chat_complete,
    )
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

/// `nativePerplexitySearch(handle, intentJson)` — shared online search.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativePerplexitySearch<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_perplexity_search,
    )
}

/// `nativeProviderModelCatalog(handle)` - shared provider model catalog.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeProviderModelCatalog<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    call_podcast_catalog_ffi(&env, handle, nmp_app_podcast_provider_model_catalog)
}

/// `nativeSpeechModelCatalog(handle)` - shared speech STT/TTS model catalog.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSpeechModelCatalog<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    call_podcast_catalog_ffi(&env, handle, nmp_app_podcast_speech_model_catalog)
}

/// `nativeLocalModelCatalog(handle)` - shared on-device model catalog.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeLocalModelCatalog<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    call_podcast_catalog_ffi(&env, handle, nmp_app_podcast_local_model_catalog)
}

/// `nativeValidateOpenRouterKey(handle)` - shared OpenRouter key validation.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeValidateOpenRouterKey<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    call_podcast_catalog_ffi(&env, handle, nmp_app_podcast_validate_openrouter_key)
}

/// `nativeValidateElevenLabsKey(handle)` - shared ElevenLabs key validation.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeValidateElevenLabsKey<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    call_podcast_catalog_ffi(&env, handle, nmp_app_podcast_validate_elevenlabs_key)
}

/// `nativeElevenLabsVoiceCatalog(handle)` - shared ElevenLabs voice catalog.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeElevenLabsVoiceCatalog<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    call_podcast_catalog_ffi(&env, handle, nmp_app_podcast_elevenlabs_voice_catalog)
}

/// `nativeElevenLabsTextToSpeech(handle, intentJson)` - shared ElevenLabs TTS.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeElevenLabsTextToSpeech<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_elevenlabs_tts_synthesize,
    )
}

/// `nativeOpenRouterWhisperTranscribe(handle, intentJson)` — shared OpenRouter STT.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeOpenRouterWhisperTranscribe<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_openrouter_whisper_transcribe,
    )
}

/// `nativeElevenLabsScribeTranscribe(handle, intentJson)` - shared ElevenLabs STT.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeElevenLabsScribeTranscribe<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_elevenlabs_scribe_transcribe,
    )
}

/// `nativeAssemblyAITranscribe(handle, intentJson)` - shared AssemblyAI STT.
/// Returns Rust's JSON envelope unchanged, or null on FFI failure.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeAssemblyAITranscribe<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    intent_json: JString<'l>,
) -> jstring {
    call_podcast_json_ffi(
        &mut env,
        handle,
        intent_json,
        nmp_app_podcast_assemblyai_transcribe,
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
