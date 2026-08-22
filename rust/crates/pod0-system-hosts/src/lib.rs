//! Real operating-system capabilities with explicit evidence and failure modes.

#![forbid(unsafe_code)]

mod credential;
mod deletion;
mod error;
mod notification;
mod secret;
mod timer;

pub use credential::{
    CredentialDeleteEvidence, CredentialKey, CredentialStore, CredentialWriteEvidence,
};
pub use deletion::{ExactFileDeleter, FileDeletionEvidence};
pub use error::{
    Capability, HostResult, InvalidInputError, NotFoundError, Operation, PermissionDeniedError,
    PermissionSource, PlatformError, SystemHostError, UnsupportedError,
};
pub use notification::{
    DesktopNotification, DesktopNotifier, NotificationBackend, NotificationEvidence,
    NotificationPermission,
};
pub use secret::Secret;
pub use timer::{
    ClockSnapshot, WakeCancellation, WakeCancelledEvidence, WakeDeadline, WakeFiredEvidence,
    WakeOutcome, WakePlan, WakeTimer,
};
