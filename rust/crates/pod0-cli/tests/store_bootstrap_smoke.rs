mod support;

#[test]
fn bootstrapped_store_opens_via_pod0_facade_open() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    support::bootstrap_authoritative_store(&store);

    let result = pod0_facade::Pod0Facade::open(store.to_string_lossy().into_owned());
    assert!(result.is_ok(), "{:?}", result.err());
}
