#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use pod0_system_hosts::{ExactFileDeleter, PermissionSource, SystemHostError};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestTree(PathBuf);

impl TestTree {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow Unix epoch")
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("system-host-tests")
            .join(format!("{label}-{}-{nonce}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("test tree should be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn deletes_only_the_requested_file_inside_an_allowed_root() {
    let tree = TestTree::new("delete-inside");
    let allowed = tree.path().join("allowed");
    let outside = tree.path().join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let allowed = fs::canonicalize(allowed).unwrap();
    let outside = fs::canonicalize(outside).unwrap();
    let target = allowed.join("target.txt");
    let neighbor = allowed.join("neighbor.txt");
    let external = outside.join("external.txt");
    fs::write(&target, b"target").unwrap();
    fs::write(&neighbor, b"neighbor").unwrap();
    fs::write(&external, b"external").unwrap();

    let deleter = ExactFileDeleter::new([allowed.clone()]).unwrap();
    let evidence = deleter.delete_exact(&target).unwrap();

    assert_eq!(evidence.requested_path, target);
    assert_eq!(evidence.allowed_root, allowed);
    assert!(!target.exists());
    assert!(neighbor.exists());
    assert!(external.exists());
}

#[test]
fn rejects_files_outside_allowed_roots() {
    let tree = TestTree::new("delete-outside");
    let allowed = tree.path().join("allowed");
    let outside = tree.path().join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let allowed = fs::canonicalize(allowed).unwrap();
    let outside = fs::canonicalize(outside).unwrap();
    let target = outside.join("keep.txt");
    fs::write(&target, b"keep").unwrap();

    let deleter = ExactFileDeleter::new([allowed]).unwrap();
    let error = deleter.delete_exact(&target).unwrap_err();

    assert!(matches!(
        error,
        SystemHostError::PermissionDenied(ref denied)
            if denied.origin == PermissionSource::Policy
    ));
    assert!(target.exists());
}

#[test]
fn rejects_noncanonical_allowed_roots() {
    let tree = TestTree::new("noncanonical-root");
    let root = tree.path().join("root");
    let child = root.join("child");
    fs::create_dir_all(&child).unwrap();
    let noncanonical = child.join("..");

    let error = ExactFileDeleter::new([noncanonical]).err().unwrap();

    assert!(matches!(error, SystemHostError::InvalidInput(_)));
}

#[test]
fn refuses_to_delete_directories() {
    let tree = TestTree::new("delete-directory");
    let allowed = tree.path().join("allowed");
    let directory = allowed.join("directory");
    fs::create_dir_all(&directory).unwrap();
    let allowed = fs::canonicalize(allowed).unwrap();

    let deleter = ExactFileDeleter::new([allowed]).unwrap();
    let error = deleter.delete_exact(&directory).unwrap_err();

    assert!(matches!(error, SystemHostError::InvalidInput(_)));
    assert!(directory.is_dir());
}

#[cfg(unix)]
#[test]
fn symlinked_parent_cannot_escape_the_allowed_root() {
    use std::os::unix::fs::symlink;

    let tree = TestTree::new("symlink-escape");
    let allowed = tree.path().join("allowed");
    let outside = tree.path().join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let allowed = fs::canonicalize(allowed).unwrap();
    let outside = fs::canonicalize(outside).unwrap();
    let victim = outside.join("victim.txt");
    fs::write(&victim, b"must survive").unwrap();
    symlink(&outside, allowed.join("escape")).unwrap();

    let deleter = ExactFileDeleter::new([allowed.clone()]).unwrap();
    let error = deleter
        .delete_exact(allowed.join("escape").join("victim.txt"))
        .unwrap_err();

    assert!(matches!(
        error,
        SystemHostError::PermissionDenied(_) | SystemHostError::Platform(_)
    ));
    assert!(victim.exists());
}

#[cfg(unix)]
#[test]
fn deleting_a_symlink_does_not_delete_its_external_target() {
    use std::os::unix::fs::symlink;

    let tree = TestTree::new("symlink-leaf");
    let allowed = tree.path().join("allowed");
    let outside = tree.path().join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let allowed = fs::canonicalize(allowed).unwrap();
    let outside = fs::canonicalize(outside).unwrap();
    let external = outside.join("external.txt");
    let link = allowed.join("external-link");
    fs::write(&external, b"survives").unwrap();
    symlink(&external, &link).unwrap();

    let deleter = ExactFileDeleter::new([allowed]).unwrap();
    deleter.delete_exact(&link).unwrap();

    assert!(!link.exists());
    assert!(external.exists());
}

#[cfg(unix)]
#[test]
fn replacing_the_root_path_does_not_replace_the_owned_capability() {
    use std::os::unix::fs::symlink;

    let tree = TestTree::new("root-substitution");
    let allowed = tree.path().join("allowed");
    let moved = tree.path().join("moved-allowed");
    let outside = tree.path().join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let allowed = fs::canonicalize(allowed).unwrap();
    let inside_target = allowed.join("victim.txt");
    let outside_target = outside.join("victim.txt");
    fs::write(&inside_target, b"owned root").unwrap();
    fs::write(&outside_target, b"outside root").unwrap();

    let deleter = ExactFileDeleter::new([allowed.clone()]).unwrap();
    fs::rename(&allowed, &moved).unwrap();
    symlink(&outside, &allowed).unwrap();

    deleter.delete_exact(allowed.join("victim.txt")).unwrap();

    assert!(!moved.join("victim.txt").exists());
    assert_eq!(fs::read(&outside_target).unwrap(), b"outside root");
}
