use pod0_domain::{CategoryItemKind, CommandId, LibraryItemId};
use sha2::{Digest as _, Sha256};

use crate::listening_import_test_support::*;
use crate::{LibraryStore, commit_listening_cutover};

pub(crate) const NOW: i64 = 1_800_000_000_000;

/// `pod0_library_commands.command_fingerprint` is checked at exactly 64
/// bytes, matching a real caller's hex-encoded digest. Tests only need a
/// value that is stable per label, so a hex digest of the label itself
/// satisfies the constraint without hand-rolling 64-character literals.
pub(crate) fn fp(label: &str) -> String {
    Sha256::digest(label.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Returns the store together with the fixture that backs it. The fixture's
/// `TempDir` must stay alive for as long as the store is used — dropping it
/// early deletes the directory the store's path still points at.
pub(crate) fn store() -> (ImportFixture, LibraryStore) {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    fixture.stage(&fixture.plan()).unwrap();
    commit_listening_cutover(&fixture.target, NOW).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    (fixture, store)
}

pub(crate) fn command(seed: u8) -> CommandId {
    CommandId::from_bytes([seed; 16])
}

pub(crate) fn item(seed: u8) -> LibraryItemId {
    LibraryItemId::from_bytes([seed; 16])
}

/// Everything resolves as a podcast — enough to exercise membership without
/// dragging the whole library fixture into every assertion.
pub(crate) fn as_podcast(_: LibraryItemId) -> Option<CategoryItemKind> {
    Some(CategoryItemKind::Podcast)
}
