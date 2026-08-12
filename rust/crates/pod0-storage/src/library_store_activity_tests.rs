use super::*;

#[test]
fn episode_starred_state_is_owned_and_replayed_by_the_library_store() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let initial = store.snapshot().unwrap().episodes[0].clone();
    let episode_id = initial.episode_id;
    let desired = !initial.is_starred;

    let revision = store
        .set_episode_starred(
            id(13),
            &"d".repeat(64),
            episode_id,
            desired,
            1_800_000_000_013,
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.episodes[0].is_starred, desired);
    assert_eq!(snapshot.playback.revision, revision);
    assert_eq!(
        store
            .set_episode_starred(
                id(13),
                &"d".repeat(64),
                episode_id,
                desired,
                1_800_000_000_014,
            )
            .unwrap(),
        revision
    );

    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(episode_id, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 2);
    assert!(matches!(
        activity.items[0].draft.fact,
        pod0_application::ActivityFact::RequestDisposition {
            disposition: pod0_application::RequestDisposition::Accepted
        }
    ));
    assert!(matches!(
        activity.items[1].draft.fact,
        pod0_application::ActivityFact::DomainTransition { .. }
    ));

    let unchanged = store
        .set_episode_starred(
            id(14),
            &"e".repeat(64),
            episode_id,
            desired,
            1_800_000_000_015,
        )
        .unwrap();
    assert_eq!(unchanged, revision);
    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(episode_id, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 3);
    assert!(matches!(
        activity.items[2].draft.fact,
        pod0_application::ActivityFact::RequestDisposition {
            disposition: pod0_application::RequestDisposition::NoSemanticChange
        }
    ));
}

#[test]
fn note_create_is_atomic_with_episode_activity_and_replay_is_exactly_once() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    prepare_empty_notes(&fixture);
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    let command_id = id(41);
    let target = Some(pod0_domain::NoteTarget::Episode {
        episode_id,
        position_milliseconds: 12_000,
    });

    let created = store
        .create_note(
            command_id,
            &"4".repeat(64),
            "Durably linked",
            pod0_domain::NoteKind::Free,
            pod0_domain::NoteAuthor::User,
            target,
            1_800_000_000_041,
        )
        .unwrap();
    let replay = store
        .create_note(
            command_id,
            &"4".repeat(64),
            "Durably linked",
            pod0_domain::NoteKind::Free,
            pod0_domain::NoteAuthor::User,
            target,
            1_800_000_000_042,
        )
        .unwrap();
    assert_eq!(replay, created);

    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(episode_id, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 2);
    assert!(matches!(
        activity.items[0].draft.fact,
        pod0_application::ActivityFact::RequestDisposition {
            disposition: pod0_application::RequestDisposition::Accepted
        }
    ));
    assert!(matches!(
        activity.items[1].draft.fact,
        pod0_application::ActivityFact::DomainTransition { .. }
    ));
}

#[test]
fn invalid_note_create_is_durably_rejected_without_mutating_notes() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    prepare_empty_notes(&fixture);
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let command_id = id(42);

    assert_eq!(
        store.create_note(
            command_id,
            &"5".repeat(64),
            "   ",
            pod0_domain::NoteKind::Free,
            pod0_domain::NoteAuthor::User,
            None,
            1_800_000_000_042,
        ),
        Err(crate::StorageError::InvalidNote)
    );
    assert!(store.note_snapshot().unwrap().notes.is_empty());

    let correlation = pod0_application::CommandActivityIdentity::new(command_id).correlation_id();
    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 1);
    assert!(matches!(
        activity.items[0].draft.fact,
        pod0_application::ActivityFact::RequestDisposition {
            disposition: pod0_application::RequestDisposition::Rejected {
                reason: pod0_application::RequestRejectionReason::Invalid
            }
        }
    ));
}

#[test]
fn note_mutations_and_rejections_remain_episode_visible_and_exactly_once() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    prepare_empty_notes(&fixture);
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    let target = Some(pod0_domain::NoteTarget::Episode {
        episode_id,
        position_milliseconds: 1_000,
    });
    let (_, note_id) = store
        .create_note(
            id(50),
            &"a".repeat(64),
            "First",
            pod0_domain::NoteKind::Free,
            pod0_domain::NoteAuthor::User,
            target,
            1_800_000_000_050,
        )
        .unwrap();
    let updated = store
        .update_note(
            id(51),
            &"b".repeat(64),
            note_id,
            pod0_domain::NoteRevision::INITIAL,
            "Second",
            pod0_domain::NoteKind::Reflection,
            target,
            1_800_000_000_051,
        )
        .unwrap();
    assert_eq!(
        store
            .update_note(
                id(51),
                &"b".repeat(64),
                note_id,
                pod0_domain::NoteRevision::INITIAL,
                "Second",
                pod0_domain::NoteKind::Reflection,
                target,
                1_800_000_000_052,
            )
            .unwrap(),
        updated
    );
    assert_eq!(
        store.update_note(
            id(52),
            &"c".repeat(64),
            note_id,
            pod0_domain::NoteRevision::INITIAL,
            "Stale",
            pod0_domain::NoteKind::Free,
            target,
            1_800_000_000_052,
        ),
        Err(crate::StorageError::RevisionConflict)
    );
    store
        .set_note_deleted(
            id(53),
            &"d".repeat(64),
            note_id,
            pod0_domain::NoteRevision::new(2),
            true,
            1_800_000_000_053,
        )
        .unwrap();
    store
        .create_note(
            id(54),
            &"e".repeat(64),
            "Clear me",
            pod0_domain::NoteKind::Free,
            pod0_domain::NoteAuthor::User,
            target,
            1_800_000_000_054,
        )
        .unwrap();
    let expected = store.note_snapshot().unwrap().revision;
    store
        .clear_notes(id(55), &"f".repeat(64), expected, 1_800_000_000_055)
        .unwrap();

    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(episode_id, None, 30)
        .unwrap();
    assert_eq!(activity.items.len(), 11);
    assert!(matches!(
        activity.items[4].draft.fact,
        pod0_application::ActivityFact::RequestDisposition {
            disposition: pod0_application::RequestDisposition::Rejected {
                reason: pod0_application::RequestRejectionReason::RevisionConflict
            }
        }
    ));
}

fn prepare_empty_notes(fixture: &ImportFixture) {
    let plan = crate::inspect_legacy_note_source(&fixture.source).unwrap();
    crate::NoteImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("notes.backup.sqlite"),
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(39),
            id(40),
        )
        .unwrap();
    crate::commit_note_cutover(&fixture.target, 1_800_000_000_001).unwrap();
}
