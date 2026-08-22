#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows",)))]
#[test]
fn non_desktop_targets_compile_against_unsupported_stubs() {
    use std::path::PathBuf;

    use pod0_system_hosts::{
        CredentialStore, DesktopNotification, DesktopNotifier, ExactFileDeleter, SystemHostError,
    };

    assert!(matches!(
        CredentialStore::new(),
        Err(SystemHostError::Unsupported(_))
    ));

    let notifier = DesktopNotifier::new("dev.pod0.unsupported-target").unwrap();
    let notification = DesktopNotification::new("Unsupported", "Unsupported target").unwrap();
    assert!(matches!(
        notifier.request_permission(),
        Err(SystemHostError::Unsupported(_))
    ));
    assert!(matches!(
        notifier.deliver(&notification),
        Err(SystemHostError::Unsupported(_))
    ));

    assert!(matches!(
        ExactFileDeleter::new([PathBuf::from("/")]),
        Err(SystemHostError::Unsupported(_))
    ));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_exact_deletion_is_honestly_unsupported() {
    use std::path::PathBuf;

    use pod0_system_hosts::{ExactFileDeleter, SystemHostError};

    assert!(matches!(
        ExactFileDeleter::new([PathBuf::from(r"C:\")]),
        Err(SystemHostError::Unsupported(_))
    ));
}
