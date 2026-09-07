use std::path::Path;

/// Create a fresh store through the production facade bootstrap.
pub fn create_authoritative_store(path: &Path) {
    drop(
        pod0_facade::Pod0Facade::create(path.to_string_lossy().into_owned())
            .expect("the production bootstrap must create a fresh authoritative store"),
    );
}
