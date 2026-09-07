use pod0_domain::CommandId;

use crate::{
    AgentStore, LibraryStore, PublicationStore, ScheduledAgentStore, StorageError, TranscriptStore,
    chapter_store_is_authoritative, create_authoritative_store,
    scheduled_agent_store_is_authoritative,
};

#[test]
fn fresh_store_initializes_every_runtime_authority_and_reopens() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pod0.sqlite");

    let store = create_authoritative_store(&path, CommandId::from_bytes([7; 16]), 123).unwrap();
    store.require_notes_authoritative().unwrap();
    drop(store);

    LibraryStore::open_authoritative(&path)
        .unwrap()
        .require_notes_authoritative()
        .unwrap();
    TranscriptStore::open_authoritative(&path).unwrap();
    ScheduledAgentStore::open_authoritative(&path).unwrap();
    AgentStore::open(&path).unwrap();
    PublicationStore::open(&path).unwrap();
    assert!(chapter_store_is_authoritative(&path).unwrap());
    assert!(scheduled_agent_store_is_authoritative(&path).unwrap());
}

#[test]
fn fresh_store_never_reinterprets_or_replaces_an_existing_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pod0.sqlite");
    std::fs::write(&path, b"existing user bytes").unwrap();

    let error = create_authoritative_store(&path, CommandId::from_bytes([9; 16]), 456)
        .expect_err("existing paths must fail closed");

    assert_eq!(error, StorageError::ImportConflict);
    assert_eq!(std::fs::read(path).unwrap(), b"existing user bytes");
}
