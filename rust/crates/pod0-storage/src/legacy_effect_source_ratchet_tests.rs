use std::path::Path;

#[test]
fn legacy_domain_execution_is_confined_to_decode_fence_and_rejection_proof() {
    let rust = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let allowed = [
        "pod0-application/src/activity_execution_contract.rs",
        "pod0-storage/src/transition_commit_cancellation_identity.rs",
        "pod0-storage/src/migration_legacy_effect_tests.rs",
        "pod0-storage/src/legacy_effect_source_ratchet_tests.rs",
    ];
    let mut violations = Vec::new();
    for crate_name in ["pod0-application", "pod0-storage", "pod0-facade"] {
        visit(&rust.join("crates").join(crate_name).join("src"), rust, &allowed, &mut violations);
    }
    assert!(violations.is_empty(), "legacy effect execution escaped its decode fence: {violations:?}");
}

fn visit(directory: &Path, rust: &Path, allowed: &[&str], violations: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, rust, allowed, violations);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            let relative = path.strip_prefix(rust.join("crates")).unwrap().to_string_lossy();
            let text = std::fs::read_to_string(&path).unwrap();
            if (text.contains("LegacyDomainDerived") || text.contains("\"DomainDerived\""))
                && !allowed.iter().any(|allowed| relative == *allowed)
            {
                violations.push(relative.into_owned());
            }
        }
    }
}
