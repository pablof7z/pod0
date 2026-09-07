use std::time::{SystemTime, UNIX_EPOCH};

use pod0_system_hosts::{CredentialKey, CredentialStore, Secret};
#[cfg(target_os = "macos")]
use pod0_system_hosts::{DesktopNotification, DesktopNotifier};

#[test]
#[ignore = "writes a temporary credential to the user's operating-system credential store"]
fn credential_round_trip_uses_the_real_operating_system_store() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let key = CredentialKey::new(
        "dev.pod0.system-hosts.integration-test",
        format!("cargo-test-{nonce}"),
    )
    .unwrap();
    let store = CredentialStore::new().unwrap();
    let secret = Secret::new(format!("credential-secret-{nonce}").into_bytes());

    store.set(&key, &secret).unwrap();
    let loaded = store.get(&key).unwrap().expect("credential should exist");
    assert_eq!(loaded.expose_secret(), secret.expose_secret());
    assert!(store.delete(&key).unwrap().is_some());
    assert!(store.get(&key).unwrap().is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn unverified_macos_application_identity_is_unsupported() {
    let notifier = DesktopNotifier::new("dev.pod0.not-the-current-test-bundle").unwrap();
    let notification =
        DesktopNotification::new("Must not be delivered", "No fallback is allowed").unwrap();

    assert!(matches!(
        notifier.request_permission(),
        Err(pod0_system_hosts::SystemHostError::Unsupported(_))
    ));
    assert!(matches!(
        notifier.deliver(&notification),
        Err(pod0_system_hosts::SystemHostError::Unsupported(_))
    ));
}
