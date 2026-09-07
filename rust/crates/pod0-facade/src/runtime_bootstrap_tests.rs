use pod0_application::{FACADE_CONTRACT_VERSION, ProjectionRequest, ProjectionScope};

use crate::Pod0Facade;

#[test]
fn facade_creates_and_reopens_the_same_authoritative_store() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pod0.sqlite");
    let path = path.to_string_lossy().into_owned();

    let created = Pod0Facade::create(path.clone()).unwrap();
    let created_projection = created.snapshot(ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 20,
    });
    drop(created);

    let reopened = Pod0Facade::open(path).unwrap();
    let reopened_projection = reopened.snapshot(ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 20,
    });

    assert_eq!(created_projection.contract_version, FACADE_CONTRACT_VERSION);
    assert_eq!(reopened_projection, created_projection);
}
