use std::sync::Arc;

use rusqlite::Connection;

use super::*;
use crate::runtime_playback_test_support::PlaybackFixture;

#[test]
fn facade_issues_one_shot_confirmation_fences_handles_and_erases_every_target() {
    let fixture = PlaybackFixture::new();
    let old_store_id = store_id(&fixture.target);
    let locations = locations(&fixture);
    let product_projection = locations
        .targets
        .iter()
        .find(|target| target.kind == UserDataErasureTargetKind::ApplicationStateProjection)
        .unwrap()
        .location
        .clone();
    std::fs::write(&product_projection, b"private projection").unwrap();

    let token = fixture
        .facade
        .prepare_erasure(
            old_store_id,
            vec![0x41; 32],
            br#"{"llmModel":"retained/model"}"#.to_vec(),
            locations.clone(),
        )
        .unwrap();
    {
        let state = fixture.facade.state();
        assert_eq!(state.erasure_lifecycle, ErasureLifecycle::Prepared);
        assert!(state.store.is_none());
        assert!(state.transcript_store.is_none());
        assert!(state.evidence_store.is_none());
    }
    let wrong = Arc::new(UserDataErasureToken {
        operation_id: CommandId::from_parts(99, 99),
    });
    assert!(matches!(
        fixture.facade.confirm_erasure(wrong),
        Err(UserDataErasureError::Conflict)
    ));
    let progress = fixture.facade.confirm_erasure(Arc::clone(&token)).unwrap();
    let fresh_store_id = finish_native(progress, locations);
    assert_ne!(fresh_store_id, old_store_id);
    assert_eq!(store_id(&fixture.target), fresh_store_id);
    assert!(!std::path::Path::new(&product_projection).exists());
    assert!(matches!(
        fixture.facade.confirm_erasure(token),
        Err(UserDataErasureError::Conflict)
    ));
    assert_eq!(
        fixture.facade.state().erasure_lifecycle,
        ErasureLifecycle::Erasing
    );
}

#[test]
fn startup_fails_closed_when_erasure_marker_exists() {
    let fixture = PlaybackFixture::new();
    let marker = fixture
        ._directory
        .path()
        .join("pod0-erasure-00000000000000000000000000000001.json");
    std::fs::write(marker, b"incomplete erasure").unwrap();
    assert!(matches!(
        Pod0Facade::open(fixture.target.to_string_lossy().into_owned()),
        Err(crate::FacadeOpenError::ErasureRecoveryRequired)
    ));
}

fn locations(fixture: &PlaybackFixture) -> UserDataErasureLocations {
    let root = fixture._directory.path();
    let recall = pod0_recall_index::recall_index_path_for_core_store(&fixture.target);
    let targets = UserDataErasureTargetKind::ALL
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let path = match kind {
                UserDataErasureTargetKind::CoreSqlite => fixture.target.clone(),
                UserDataErasureTargetKind::CoreWal => suffixed(&fixture.target, "-wal"),
                UserDataErasureTargetKind::CoreShm => suffixed(&fixture.target, "-shm"),
                UserDataErasureTargetKind::RecallIndex => recall.clone(),
                UserDataErasureTargetKind::RecallIndexWal => suffixed(&recall, "-wal"),
                UserDataErasureTargetKind::RecallIndexShm => suffixed(&recall, "-shm"),
                _ => root.join(format!("product-target-{index}")),
            };
            UserDataErasureTargetLocation {
                kind,
                location: kind
                    .into_storage_kind()
                    .native_action_identifier()
                    .map_or_else(
                        || {
                            kind.into_storage_kind().covering_kind().map_or_else(
                                || path.to_string_lossy().into_owned(),
                                |_| String::new(),
                            )
                        },
                        str::to_owned,
                    ),
                covered_by: kind.into_storage_kind().covering_kind().map(storage_kind),
            }
        })
        .collect();
    UserDataErasureLocations {
        recovery_root: root.to_string_lossy().into_owned(),
        allowed_roots: vec![root.to_string_lossy().into_owned()],
        targets,
    }
}

fn finish_native(
    mut progress: UserDataErasureResult,
    locations: UserDataErasureLocations,
) -> CommandId {
    loop {
        match progress {
            UserDataErasureResult::Complete { fresh_store_id } => return fresh_store_id,
            UserDataErasureResult::AwaitingNativeActions { actions } => {
                let action = actions.into_iter().next().expect("pending native action");
                progress = record_native_erasure_observation(
                    locations.clone(),
                    action.action_id,
                    action.attempt,
                    true,
                )
                .unwrap();
            }
        }
    }
}

impl UserDataErasureTargetKind {
    fn into_storage_kind(self) -> pod0_storage::UserDataTargetKind {
        self.into()
    }
}

fn storage_kind(kind: pod0_storage::UserDataTargetKind) -> UserDataErasureTargetKind {
    kind.into()
}

fn suffixed(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}{suffix}", path.to_string_lossy()))
}

fn store_id(path: &std::path::Path) -> CommandId {
    let bytes: Vec<u8> = Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT store_id FROM pod0_store_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    CommandId::from_bytes(bytes.try_into().unwrap())
}
