use std::time::SystemTime;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
use crate::PermissionSource;
use crate::{Capability, HostResult, Operation, Secret, SystemHostError};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
use keyring_core::api::CredentialStoreApi;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
use std::sync::Arc;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
use zeroize::Zeroize;

const CAPABILITY: Capability = Capability::CredentialStore;

#[cfg(target_os = "linux")]
type NativeCredentialStore = zbus_secret_service_keyring_store::Store;
#[cfg(target_os = "macos")]
type NativeCredentialStore = apple_native_keyring_store::keychain::Store;
#[cfg(target_os = "windows")]
type NativeCredentialStore = windows_native_keyring_store::Store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialKey {
    service: String,
    account: String,
}

impl CredentialKey {
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> HostResult<Self> {
        let service = validated_label("service", service.into())?;
        let account = validated_label("account", account.into())?;
        Ok(Self { service, account })
    }

    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }

    #[must_use]
    pub fn account(&self) -> &str {
        &self.account
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialWriteEvidence {
    pub completed_at: SystemTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialDeleteEvidence {
    pub completed_at: SystemTime,
}

#[derive(Clone, Debug)]
pub struct CredentialStore {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
    backend: Arc<NativeCredentialStore>,
}

impl CredentialStore {
    pub fn new() -> HostResult<Self> {
        Self::new_impl()
    }

    pub fn get(&self, key: &CredentialKey) -> HostResult<Option<Secret>> {
        self.get_impl(key)
    }

    pub fn set(&self, key: &CredentialKey, secret: &Secret) -> HostResult<CredentialWriteEvidence> {
        self.set_impl(key, secret)
    }

    pub fn delete(&self, key: &CredentialKey) -> HostResult<Option<CredentialDeleteEvidence>> {
        self.delete_impl(key)
    }
}

fn validated_label(field: &str, value: String) -> HostResult<String> {
    if value.is_empty() {
        return Err(SystemHostError::invalid(
            CAPABILITY,
            Operation::ConfigureCredential,
            format!("{field} must not be empty"),
        ));
    }
    if value.contains('\0') {
        return Err(SystemHostError::invalid(
            CAPABILITY,
            Operation::ConfigureCredential,
            format!("{field} must not contain NUL"),
        ));
    }
    Ok(value)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
impl CredentialStore {
    fn new_impl() -> HostResult<Self> {
        NativeCredentialStore::new()
            .map(|backend| Self { backend })
            .map_err(|error| map_credential_error(error, Operation::ConfigureCredential))
    }

    fn entry(&self, key: &CredentialKey, operation: Operation) -> HostResult<keyring_core::Entry> {
        self.backend
            .build(key.service(), key.account(), None)
            .map_err(|error| map_credential_error(error, operation))
    }

    fn get_impl(&self, key: &CredentialKey) -> HostResult<Option<Secret>> {
        let entry = self.entry(key, Operation::GetCredential)?;
        match entry.get_secret() {
            Ok(secret) => Ok(Some(Secret::new(secret))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_credential_error(error, Operation::GetCredential)),
        }
    }

    fn set_impl(
        &self,
        key: &CredentialKey,
        secret: &Secret,
    ) -> HostResult<CredentialWriteEvidence> {
        let entry = self.entry(key, Operation::SetCredential)?;
        entry
            .set_secret(secret.expose_secret())
            .map_err(|error| map_credential_error(error, Operation::SetCredential))?;
        Ok(CredentialWriteEvidence {
            completed_at: SystemTime::now(),
        })
    }

    fn delete_impl(&self, key: &CredentialKey) -> HostResult<Option<CredentialDeleteEvidence>> {
        let entry = self.entry(key, Operation::DeleteCredential)?;
        match entry.delete_credential() {
            Ok(()) => Ok(Some(CredentialDeleteEvidence {
                completed_at: SystemTime::now(),
            })),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_credential_error(error, Operation::DeleteCredential)),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
fn map_credential_error(error: keyring_core::Error, operation: Operation) -> SystemHostError {
    match error {
        keyring_core::Error::NoStorageAccess(_) => {
            SystemHostError::permission(CAPABILITY, operation, PermissionSource::OperatingSystem)
        }
        keyring_core::Error::NoEntry => SystemHostError::not_found(CAPABILITY, operation),
        keyring_core::Error::Invalid(field, reason) => SystemHostError::invalid(
            CAPABILITY,
            operation,
            format!("{field} is invalid: {reason}"),
        ),
        keyring_core::Error::TooLong(field, limit) => SystemHostError::invalid(
            CAPABILITY,
            operation,
            format!("{field} exceeds platform limit {limit}"),
        ),
        keyring_core::Error::NotSupportedByStore(_) => SystemHostError::unsupported(CAPABILITY),
        keyring_core::Error::NoDefaultStore => SystemHostError::platform(
            CAPABILITY,
            operation,
            "credential store initialization failed",
        ),
        keyring_core::Error::PlatformFailure(error) => {
            SystemHostError::platform(CAPABILITY, operation, error.to_string())
        }
        keyring_core::Error::BadDataFormat(mut secret, error) => {
            secret.zeroize();
            SystemHostError::platform(CAPABILITY, operation, error.to_string())
        }
        keyring_core::Error::BadEncoding(mut secret) => {
            secret.zeroize();
            SystemHostError::platform(CAPABILITY, operation, "credential encoding is invalid")
        }
        keyring_core::Error::BadStoreFormat(reason) => {
            SystemHostError::platform(CAPABILITY, operation, reason)
        }
        keyring_core::Error::Ambiguous(_) => {
            SystemHostError::platform(CAPABILITY, operation, "multiple credentials matched")
        }
        _ => SystemHostError::platform(CAPABILITY, operation, "unclassified credential failure"),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows",)))]
impl CredentialStore {
    fn new_impl() -> HostResult<Self> {
        Err(SystemHostError::unsupported(CAPABILITY))
    }

    fn get_impl(&self, _key: &CredentialKey) -> HostResult<Option<Secret>> {
        Err(SystemHostError::unsupported(CAPABILITY))
    }

    fn set_impl(
        &self,
        _key: &CredentialKey,
        _secret: &Secret,
    ) -> HostResult<CredentialWriteEvidence> {
        Err(SystemHostError::unsupported(CAPABILITY))
    }

    fn delete_impl(&self, _key: &CredentialKey) -> HostResult<Option<CredentialDeleteEvidence>> {
        Err(SystemHostError::unsupported(CAPABILITY))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
    use super::NativeCredentialStore;

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows",))]
    #[test]
    fn selects_a_concrete_native_backend_at_compile_time() {
        let selected = std::any::type_name::<NativeCredentialStore>();

        #[cfg(target_os = "linux")]
        assert_eq!(selected, "zbus_secret_service_keyring_store::store::Store");
        #[cfg(target_os = "macos")]
        assert_eq!(selected, "apple_native_keyring_store::keychain::Store");
        #[cfg(target_os = "windows")]
        assert_eq!(selected, "windows_native_keyring_store::store::Store");
    }
}
