//! Capability-dispatch helpers for [`PodcastHostOpHandler`].
//!
//! Each method serializes a per-namespace command and forwards it through
//! `*mut NmpApp::dispatch_capability` back into the iOS executor. A
//! null/uninitialized app pointer (unit tests, pre-`nmp_app_start`) degrades
//! to a no-op rather than dereferencing null (the "D6" guard).

use nmp_core::substrate::CapabilityRequest;

use crate::capability::{
    notification_command_json, AudioCommand, DownloadCommand, NotificationCommand,
    AUDIO_CAPABILITY_NAMESPACE, DOWNLOAD_CAPABILITY_NAMESPACE, NOTIFICATION_CAPABILITY_NAMESPACE,
};
use crate::host_op_handler::PodcastHostOpHandler;
use podcast_feeds::http::{
    HttpCommand, HttpRequest, HttpResult, HTTP_ASYNC_CAPABILITY_NAMESPACE, HTTP_CAPABILITY_NAMESPACE,
};

impl PodcastHostOpHandler {
    pub(crate) fn dispatch_http(
        &self,
        req: &HttpRequest,
        correlation_id: &str,
    ) -> Result<HttpResult, String> {
        let payload_json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        let cap_req = CapabilityRequest {
            namespace: HTTP_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let envelope = unsafe { &*self.app }.dispatch_capability(&cap_req);
        serde_json::from_str::<HttpResult>(&envelope.result_json)
            .map_err(|e| format!("decode http result: {e}"))
    }

    /// Fire-and-forget HTTP dispatch on the **async** capability path. Unlike
    /// [`Self::dispatch_http`], this does not block the actor thread waiting for
    /// the round-trip: it emits an [`HttpCommand`] bearing `request_id` and
    /// returns immediately (the synchronous envelope is a bare ack). The
    /// platform runs the transport off-thread and reports the result back via
    /// the HTTP-report FFI ([`crate::ffi::http_report`]), which resolves the
    /// pending request through the [`crate::feed_fetch::FeedFetchCoordinator`].
    ///
    /// D6 — a null app pointer (unit tests / pre-`nmp_app_start`) degrades to a
    /// no-op rather than dereferencing null, mirroring `dispatch_audio`.
    pub(crate) fn dispatch_http_async(&self, request_id: &str, request: HttpRequest) {
        if self.app.is_null() {
            return;
        }
        let command = HttpCommand {
            request_id: request_id.to_owned(),
            request,
        };
        let Ok(payload_json) = serde_json::to_string(&command) else {
            return;
        };
        let cap_req = CapabilityRequest {
            namespace: HTTP_ASYNC_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: request_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&cap_req);
    }

    pub(crate) fn dispatch_audio(
        &self,
        cmd: &AudioCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        // D6: a null/uninitialized app pointer (unit tests, pre-`nmp_app_start`)
        // degrades to a no-op rather than dereferencing null. Mirrors the
        // publish path's null guard.
        if self.app.is_null() {
            return Ok(());
        }
        let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let req = CapabilityRequest {
            namespace: AUDIO_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    pub(super) fn dispatch_download(
        &self,
        cmd: &DownloadCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        // D6: null/uninitialized app pointer degrades to a no-op (see
        // `dispatch_audio`).
        if self.app.is_null() {
            return Ok(());
        }
        let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let req = CapabilityRequest {
            namespace: DOWNLOAD_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    /// Fire-and-forget notification dispatch. Mirrors the audio/download
    /// envelope shape so the iOS-side router can fan out by namespace
    /// without special-casing.
    pub(super) fn dispatch_notification(
        &self,
        cmd: &NotificationCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        // D6: null/uninitialized app pointer degrades to a no-op (see
        // `dispatch_audio`).
        if self.app.is_null() {
            return Ok(());
        }
        let payload_json = notification_command_json(cmd);
        let req = CapabilityRequest {
            namespace: NOTIFICATION_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }
}

#[cfg(test)]
#[path = "speed_persist_tests.rs"]
mod speed_persist_tests;
