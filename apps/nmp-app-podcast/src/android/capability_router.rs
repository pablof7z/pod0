use std::ffi::{c_char, c_void, CStr, CString};

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::jlong;
use jni::JNIEnv;
use jni::JavaVM;
use nmp_ffi::nmp_app_set_capability_callback;

pub(super) struct AndroidCapabilityContext {
    vm: JavaVM,
    router: GlobalRef,
}

fn capability_error_envelope(message: &str) -> *mut c_char {
    let json = format!(
        "{{\"namespace\":\"\",\"correlation_id\":\"\",\"result_json\":\"{{\\\"status\\\":\\\"error\\\",\\\"message\\\":\\\"{message}\\\"}}\"}}"
    );
    CString::new(json)
        .unwrap_or_else(|_| CString::new("{}").expect("static JSON has no NUL"))
        .into_raw()
}

extern "C" fn android_capability_callback(
    context: *mut c_void,
    request_json: *const c_char,
) -> *mut c_char {
    if context.is_null() || request_json.is_null() {
        return capability_error_envelope("null-args");
    }
    // SAFETY: registered by nativeSetCapabilityRouter and cleared before drop.
    let ctx = unsafe { &*(context as *const AndroidCapabilityContext) };
    let request = match unsafe { CStr::from_ptr(request_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return capability_error_envelope("bad-utf8"),
    };
    let mut env = match ctx.vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => return capability_error_envelope("attach-failed"),
    };
    let j_request = match env.new_string(request) {
        Ok(s) => s,
        Err(_) => return capability_error_envelope("string-failed"),
    };
    let j_request_obj = JObject::from(j_request);
    let result = match env.call_method(
        ctx.router.as_obj(),
        "handle",
        "(Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&j_request_obj)],
    ) {
        Ok(value) => value,
        Err(_) => return capability_error_envelope("router-call-failed"),
    };
    let obj = match result.l() {
        Ok(obj) if !obj.is_null() => obj,
        _ => return capability_error_envelope("router-returned-null"),
    };
    let response = match env.get_string(&JString::from(obj)) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return capability_error_envelope("response-utf8-failed"),
    };
    CString::new(response)
        .unwrap_or_else(|_| CString::new("{}").expect("static JSON has no NUL"))
        .into_raw()
}

pub(super) fn clear_capability_router(session: &super::Session) {
    nmp_app_set_capability_callback(session.app, std::ptr::null_mut(), None);
    if let Ok(mut slot) = session.capability_ctx.lock() {
        if let Some(ctx) = slot.take() {
            // SAFETY: allocated with Box::into_raw in nativeSetCapabilityRouter.
            unsafe {
                drop(Box::from_raw(ctx));
            }
        }
    }
}

/// `nativeSetCapabilityRouter(handle, router)` — register or clear Android's
/// `CapabilityRequest` router.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSetCapabilityRouter<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    router: JObject<'l>,
) {
    let Some(s) = super::session_ref(handle) else {
        return;
    };
    clear_capability_router(s);
    if router.is_null() {
        return;
    }
    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(_) => return,
    };
    let global = match env.new_global_ref(router) {
        Ok(g) => g,
        Err(_) => return,
    };
    let ctx = Box::into_raw(Box::new(AndroidCapabilityContext { vm, router: global }));
    nmp_app_set_capability_callback(s.app, ctx as *mut c_void, Some(android_capability_callback));
    if let Ok(mut slot) = s.capability_ctx.lock() {
        *slot = Some(ctx);
    } else {
        nmp_app_set_capability_callback(s.app, std::ptr::null_mut(), None);
        // SAFETY: the callback has been cleared, so reclaim the new box.
        unsafe {
            drop(Box::from_raw(ctx));
        }
    }
}
