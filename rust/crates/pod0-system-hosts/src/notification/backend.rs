#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::time::SystemTime;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use super::NotificationBackend;
use super::{CAPABILITY, DesktopNotification, NotificationEvidence, NotificationPermission};
use crate::{HostResult, SystemHostError};
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use crate::{Operation, PermissionSource};

#[cfg(target_os = "macos")]
pub(super) fn request_permission(application_id: &str) -> HostResult<NotificationPermission> {
    verify_macos_application_identity(application_id, Operation::AuthorizeNotification)?;
    mac_usernotifications::blocking::request_auth()
        .map(|granted| {
            if granted {
                NotificationPermission::Granted
            } else {
                NotificationPermission::Denied
            }
        })
        .map_err(|error| map_macos_error(error, Operation::AuthorizeNotification))
}

#[cfg(target_os = "macos")]
fn verify_macos_application_identity(application_id: &str, operation: Operation) -> HostResult<()> {
    use objc2_foundation::NSBundle;

    let Some(bundle_identifier) = NSBundle::mainBundle().bundleIdentifier() else {
        return Err(SystemHostError::unsupported(CAPABILITY));
    };
    if bundle_identifier.to_string() != application_id {
        return Err(SystemHostError::unsupported(CAPABILITY));
    }
    mac_usernotifications::check_bundle().map_err(|error| match error {
        mac_usernotifications::Error::NoBundleIdentifier => {
            SystemHostError::unsupported(CAPABILITY)
        }
        other => map_macos_error(other, operation),
    })
}

#[cfg(target_os = "macos")]
pub(super) fn deliver(
    application_id: &str,
    notification: &DesktopNotification,
) -> HostResult<NotificationEvidence> {
    verify_macos_application_identity(application_id, Operation::DeliverNotification)?;
    deliver_macos(notification)
}

#[cfg(target_os = "macos")]
fn deliver_macos(notification: &DesktopNotification) -> HostResult<NotificationEvidence> {
    use mac_usernotifications::AuthorizationStatus;

    let settings = mac_usernotifications::blocking::get_notification_settings()
        .map_err(|error| map_macos_error(error, Operation::DeliverNotification))?;
    match settings.authorization_status {
        AuthorizationStatus::Authorized
        | AuthorizationStatus::Provisional
        | AuthorizationStatus::Ephemeral => {}
        AuthorizationStatus::Denied | AuthorizationStatus::NotDetermined => {
            return Err(SystemHostError::permission(
                CAPABILITY,
                Operation::DeliverNotification,
                PermissionSource::OperatingSystem,
            ));
        }
        AuthorizationStatus::Unknown => {
            return Err(SystemHostError::platform(
                CAPABILITY,
                Operation::DeliverNotification,
                "macOS returned an unknown notification authorization status",
            ));
        }
    }

    let handle = mac_usernotifications::Notification::new()
        .title(notification.title())
        .message(notification.body())
        .send_blocking()
        .map_err(|error| map_macos_error(error, Operation::DeliverNotification))?;
    Ok(NotificationEvidence {
        submitted_at: SystemTime::now(),
        backend: NotificationBackend::MacUserNotifications,
        native_id: Some(handle.notification_id().to_owned()),
    })
}

#[cfg(target_os = "macos")]
fn map_macos_error(error: mac_usernotifications::Error, operation: Operation) -> SystemHostError {
    match error {
        mac_usernotifications::Error::NoBundleIdentifier => {
            SystemHostError::unsupported(CAPABILITY)
        }
        other => SystemHostError::platform(CAPABILITY, operation, other.to_string()),
    }
}

#[cfg(target_os = "linux")]
pub(super) fn request_permission(_application_id: &str) -> HostResult<NotificationPermission> {
    Ok(NotificationPermission::NotRequired)
}

#[cfg(target_os = "linux")]
pub(super) fn deliver(
    application_id: &str,
    notification: &DesktopNotification,
) -> HostResult<NotificationEvidence> {
    let mut native = notify_rust::Notification::new();
    native
        .appname(application_id)
        .summary(notification.title())
        .body(notification.body());
    let handle = native.show().map_err(map_notify_error)?;
    Ok(NotificationEvidence {
        submitted_at: SystemTime::now(),
        backend: NotificationBackend::Freedesktop,
        native_id: Some(handle.id().to_string()),
    })
}

#[cfg(target_os = "windows")]
pub(super) fn request_permission(_application_id: &str) -> HostResult<NotificationPermission> {
    Ok(NotificationPermission::NotRequired)
}

#[cfg(target_os = "windows")]
pub(super) fn deliver(
    application_id: &str,
    notification: &DesktopNotification,
) -> HostResult<NotificationEvidence> {
    let mut native = notify_rust::Notification::new();
    native
        .app_id(application_id)
        .summary(notification.title())
        .body(notification.body());
    let _handle = native.show().map_err(map_notify_error)?;
    Ok(NotificationEvidence {
        submitted_at: SystemTime::now(),
        backend: NotificationBackend::WindowsToast,
        native_id: None,
    })
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn map_notify_error(error: notify_rust::error::Error) -> SystemHostError {
    let detail = error.to_string();
    let normalized = detail.to_ascii_lowercase();
    if normalized.contains("access denied")
        || normalized.contains("permission denied")
        || normalized.contains("not authorized")
        || normalized.contains("0x80070005")
    {
        SystemHostError::permission(
            CAPABILITY,
            Operation::DeliverNotification,
            PermissionSource::OperatingSystem,
        )
    } else {
        SystemHostError::platform(CAPABILITY, Operation::DeliverNotification, detail)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(super) fn request_permission(_application_id: &str) -> HostResult<NotificationPermission> {
    Err(SystemHostError::unsupported(CAPABILITY))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(super) fn deliver(
    _application_id: &str,
    _notification: &DesktopNotification,
) -> HostResult<NotificationEvidence> {
    Err(SystemHostError::unsupported(CAPABILITY))
}
