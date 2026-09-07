use std::{fmt, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    CredentialStore,
    DesktopNotification,
    WakeTimer,
    ExactFileDeletion,
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CredentialStore => "credential store",
            Self::DesktopNotification => "desktop notification",
            Self::WakeTimer => "wake timer",
            Self::ExactFileDeletion => "exact file deletion",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    ConfigureCredential,
    GetCredential,
    SetCredential,
    DeleteCredential,
    AuthorizeNotification,
    DeliverNotification,
    ScheduleWake,
    WaitForWake,
    ConfigureDeletion,
    DeleteFile,
}

impl fmt::Display for Operation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ConfigureCredential => "configure credential",
            Self::GetCredential => "get credential",
            Self::SetCredential => "set credential",
            Self::DeleteCredential => "delete credential",
            Self::AuthorizeNotification => "authorize notification",
            Self::DeliverNotification => "deliver notification",
            Self::ScheduleWake => "schedule wake",
            Self::WaitForWake => "wait for wake",
            Self::ConfigureDeletion => "configure deletion",
            Self::DeleteFile => "delete file",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionSource {
    OperatingSystem,
    Policy,
}

#[derive(Debug, thiserror::Error)]
#[error("{capability} is unsupported on {platform}")]
pub struct UnsupportedError {
    pub capability: Capability,
    pub platform: &'static str,
}

#[derive(Debug, thiserror::Error)]
#[error("{origin:?} permission denied while attempting to {operation}")]
pub struct PermissionDeniedError {
    pub capability: Capability,
    pub operation: Operation,
    pub origin: PermissionSource,
}

#[derive(Debug, thiserror::Error)]
#[error("{capability} platform failure while attempting to {operation}: {detail}")]
pub struct PlatformError {
    pub capability: Capability,
    pub operation: Operation,
    pub platform: &'static str,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid input for {capability} while attempting to {operation}: {detail}")]
pub struct InvalidInputError {
    pub capability: Capability,
    pub operation: Operation,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
#[error("{capability} resource was not found while attempting to {operation}")]
pub struct NotFoundError {
    pub capability: Capability,
    pub operation: Operation,
}

#[derive(Debug, thiserror::Error)]
pub enum SystemHostError {
    #[error(transparent)]
    Unsupported(#[from] UnsupportedError),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDeniedError),
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    InvalidInput(#[from] InvalidInputError),
    #[error(transparent)]
    NotFound(#[from] NotFoundError),
}

pub type HostResult<T> = Result<T, SystemHostError>;

impl SystemHostError {
    pub(crate) fn unsupported(capability: Capability) -> Self {
        UnsupportedError {
            capability,
            platform: platform_name(),
        }
        .into()
    }

    pub(crate) fn permission(
        capability: Capability,
        operation: Operation,
        source: PermissionSource,
    ) -> Self {
        PermissionDeniedError {
            capability,
            operation,
            origin: source,
        }
        .into()
    }

    pub(crate) fn platform(
        capability: Capability,
        operation: Operation,
        detail: impl Into<String>,
    ) -> Self {
        PlatformError {
            capability,
            operation,
            platform: platform_name(),
            detail: detail.into(),
        }
        .into()
    }

    pub(crate) fn invalid(
        capability: Capability,
        operation: Operation,
        detail: impl Into<String>,
    ) -> Self {
        InvalidInputError {
            capability,
            operation,
            detail: detail.into(),
        }
        .into()
    }

    pub(crate) fn not_found(capability: Capability, operation: Operation) -> Self {
        NotFoundError {
            capability,
            operation,
        }
        .into()
    }

    pub(crate) fn from_io(capability: Capability, operation: Operation, error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::PermissionDenied => {
                Self::permission(capability, operation, PermissionSource::OperatingSystem)
            }
            io::ErrorKind::NotFound => Self::not_found(capability, operation),
            io::ErrorKind::InvalidInput | io::ErrorKind::InvalidFilename => {
                Self::invalid(capability, operation, error.to_string())
            }
            io::ErrorKind::Unsupported => Self::unsupported(capability),
            _ => Self::platform(capability, operation, error.to_string()),
        }
    }
}

pub(crate) const fn platform_name() -> &'static str {
    std::env::consts::OS
}
