use crate::runtime_playback_test_support::{PlaybackFixture, dispatch, playback};
use crate::*;

#[test]
fn typed_navigation_uses_the_selected_artifact_and_cannot_replay_a_seek() {
    let fixture = PlaybackFixture::new_with_chapters();
    fixture.dispatch(100, PlaybackCommand::Restore);
    let _ = fixture.facade.next_host_requests(u16::MAX);
    let context = fixture
        .playback()
        .current
        .unwrap()
        .chapter_context
        .expect("selected chapter context");
    assert_eq!(
        context.policy_version,
        pod0_domain::CHAPTER_PLAYBACK_POLICY_VERSION
    );

    fixture.dispatch(
        101,
        PlaybackCommand::NextChapter {
            context,
            position_milliseconds: 0,
        },
    );
    let requests = fixture.facade.next_host_requests(u16::MAX);
    assert_eq!(chapter_seeks(&requests).len(), 1);
    assert!(matches!(
        chapter_seeks(&requests)[0].request,
        HostRequest::Seek {
            episode_id,
            position_milliseconds: 60_000,
            reason: PlaybackSeekReason::NextChapter,
            chapter_context: Some(value),
        } if episode_id == fixture.episode_id && value == context
    ));
    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        60_000
    );

    fixture.dispatch(
        101,
        PlaybackCommand::NextChapter {
            context,
            position_milliseconds: 0,
        },
    );
    assert!(fixture.facade.next_host_requests(u16::MAX).is_empty());

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    dispatch(&reopened, 100, PlaybackCommand::Restore);
    let _ = reopened.next_host_requests(u16::MAX);
    assert_eq!(
        playback(&reopened).current.unwrap().chapter_context,
        Some(context),
        "replaying the restore command reconstructs the same deterministic session fence"
    );
    dispatch(
        &reopened,
        101,
        PlaybackCommand::NextChapter {
            context,
            position_milliseconds: 0,
        },
    );
    assert!(
        chapter_seeks(&reopened.next_host_requests(u16::MAX)).is_empty(),
        "the durable command receipt must suppress a repeated native seek"
    );
}

#[test]
fn previous_navigation_preserves_restart_semantics_and_rejects_stale_sessions() {
    let fixture = PlaybackFixture::new_with_chapters();
    fixture.dispatch(110, PlaybackCommand::Restore);
    let _ = fixture.facade.next_host_requests(u16::MAX);
    let context = fixture.playback().current.unwrap().chapter_context.unwrap();

    fixture.dispatch(
        111,
        PlaybackCommand::PreviousChapter {
            context,
            position_milliseconds: 61_000,
        },
    );
    let requests = fixture.facade.next_host_requests(u16::MAX);
    assert!(matches!(
        chapter_seeks(&requests).single().request,
        HostRequest::Seek {
            position_milliseconds: 0,
            reason: PlaybackSeekReason::PreviousChapter,
            ..
        }
    ));

    fixture.dispatch(
        112,
        PlaybackCommand::PreviousChapter {
            context,
            position_milliseconds: 64_000,
        },
    );
    let requests = fixture.facade.next_host_requests(u16::MAX);
    assert!(matches!(
        chapter_seeks(&requests).single().request,
        HostRequest::Seek {
            position_milliseconds: 60_000,
            reason: PlaybackSeekReason::PreviousChapterRestart,
            ..
        }
    ));

    let stale = ChapterPlaybackContext {
        session_id: ChapterPlaybackSessionId::from_parts(99, 99),
        ..context
    };
    fixture.dispatch(
        113,
        PlaybackCommand::NextChapter {
            context: stale,
            position_milliseconds: 0,
        },
    );
    assert!(chapter_seeks(&fixture.facade.next_host_requests(u16::MAX)).is_empty());
    let playback = fixture.playback();
    let operation = playback
        .operations
        .iter()
        .find(|operation| operation.command_id == CommandId::from_parts(10, 113))
        .unwrap();
    assert_eq!(operation.stage, OperationStage::Failed);
    assert_eq!(
        operation.failure.as_ref().unwrap().code,
        CoreFailureCode::RevisionConflict
    );
}

fn chapter_seeks(requests: &[HostRequestEnvelope]) -> Vec<&HostRequestEnvelope> {
    requests
        .iter()
        .filter(|request| {
            matches!(
                request.request,
                HostRequest::Seek {
                    chapter_context: Some(_),
                    ..
                }
            )
        })
        .collect()
}

trait Single<T> {
    fn single(&self) -> T;
}

impl<T: Copy> Single<T> for Vec<T> {
    fn single(&self) -> T {
        assert_eq!(self.len(), 1);
        self[0]
    }
}
