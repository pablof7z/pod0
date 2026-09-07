use std::time::SystemTime;

use crate::{Capability, HostResult, Operation, SystemHostError};

mod backend;

const CAPABILITY: Capability = Capability::DesktopNotification;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopNotification {
    title: String,
    body: String,
}

impl DesktopNotification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> HostResult<Self> {
        let title = title.into();
        if title.is_empty() {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::DeliverNotification,
                "title must not be empty",
            ));
        }
        if title.contains('\0') {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::DeliverNotification,
                "title must not contain NUL",
            ));
        }
        let body = body.into();
        if body.contains('\0') {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::DeliverNotification,
                "body must not contain NUL",
            ));
        }
        Ok(Self { title, body })
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPermission {
    Granted,
    Denied,
    NotRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationBackend {
    Freedesktop,
    MacUserNotifications,
    WindowsToast,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationEvidence {
    pub submitted_at: SystemTime,
    pub backend: NotificationBackend,
    pub native_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DesktopNotifier {
    application_id: String,
}

impl DesktopNotifier {
    pub fn new(application_id: impl Into<String>) -> HostResult<Self> {
        let application_id = application_id.into();
        if application_id.is_empty() || application_id.contains('\0') {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::DeliverNotification,
                "application identifier must be non-empty and contain no NUL",
            ));
        }
        Ok(Self { application_id })
    }

    pub fn request_permission(&self) -> HostResult<NotificationPermission> {
        backend::request_permission(&self.application_id)
    }

    pub fn deliver(&self, notification: &DesktopNotification) -> HostResult<NotificationEvidence> {
        backend::deliver(&self.application_id, notification)
    }
}
