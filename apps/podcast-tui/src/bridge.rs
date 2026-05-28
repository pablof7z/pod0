use std::sync::mpsc::{self, Receiver, Sender};

use nmp_ffi::NmpApp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NmpEvent {
    pub payload: String,
}

pub struct NmpUpdateBridge {
    tx: Sender<NmpEvent>,
}

impl NmpUpdateBridge {
    #[must_use]
    pub fn channel() -> (Box<Self>, Receiver<NmpEvent>) {
        let (tx, rx) = mpsc::channel();
        (Box::new(Self { tx }), rx)
    }

    pub fn register(app: *mut NmpApp, bridge: &mut Box<Self>) {
        let context = bridge.as_mut() as *mut Self as *mut std::ffi::c_void;
        nmp_ffi::nmp_app_set_update_callback(app, context, Some(on_update));
    }
}

pub fn unregister(app: *mut NmpApp) {
    nmp_ffi::nmp_app_set_update_callback(app, std::ptr::null_mut(), None);
}

extern "C" fn on_update(context: *mut std::ffi::c_void, payload: *const u8, len: usize) {
    if context.is_null() || payload.is_null() {
        return;
    }
    let bridge = unsafe { &*(context as *const NmpUpdateBridge) };
    let bytes = unsafe { std::slice::from_raw_parts(payload, len) };
    let payload = String::from_utf8_lossy(bytes).into_owned();
    let _ = bridge.tx.send(NmpEvent { payload });
}
