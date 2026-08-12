use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityDomain, ActivityFact, ActivitySubject, DownloadAdmissionActivityInput,
    DownloadDispositionActivityInput, DownloadIntentOrigin, DurableInternalCommandRequest,
    InternalCommandKind, RequestDisposition, plan_download_admission, plan_download_noop,
};

#[test]
fn admitted_download_records_the_typed_attempt_transition() {
    let plan = plan_download_admission(DownloadAdmissionActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id: EpisodeId::from_parts(3, 4),
        current_revision: StateRevision::new(5),
        legacy_replay: false,
        state_changes: true,
        admitted: true,
        origin: DownloadIntentOrigin::User,
    })
    .unwrap();
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
    assert!(effects.is_empty());
}

#[test]
fn automatic_batch_authorizes_each_episode_as_a_durable_internal_command() {
    let first = EpisodeId::from_parts(30, 31);
    let second = EpisodeId::from_parts(32, 33);
    let request = |episode_id| DurableInternalCommandRequest {
        kind: InternalCommandKind::RequestEpisodeDownload {
            origin: DownloadIntentOrigin::Automatic,
        },
        target: ActivityDomain::Download,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
    };
    let plan = plan_download_noop(DownloadDispositionActivityInput {
        command_id: CommandId::from_parts(34, 35),
        subject: ActivitySubject::Global,
        episode_id: None,
        current_revision: StateRevision::new(36),
        legacy_replay: false,
        origin: DownloadIntentOrigin::Automatic,
        internal_commands: vec![request(first), request(second)],
    })
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, _, _, facts, _, commands, _) = plan.into_parts();
    assert_eq!(facts.len(), 3);
    assert_eq!(commands.len(), 2);
}

#[test]
fn obsolete_or_empty_download_request_is_still_a_durable_disposition() {
    let episode_id = EpisodeId::from_parts(20, 21);
    let plan = plan_download_noop(DownloadDispositionActivityInput {
        command_id: CommandId::from_parts(22, 23),
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        current_revision: StateRevision::new(24),
        legacy_replay: false,
        origin: DownloadIntentOrigin::Automatic,
        internal_commands: Vec::new(),
    })
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::NoSemanticChange);
    let (_, _, _, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn active_workflow_is_a_no_change_disposition_without_an_effect() {
    let plan = plan_download_admission(DownloadAdmissionActivityInput {
        command_id: CommandId::from_parts(10, 11),
        episode_id: EpisodeId::from_parts(12, 13),
        current_revision: StateRevision::new(14),
        legacy_replay: false,
        state_changes: false,
        admitted: true,
        origin: DownloadIntentOrigin::Playback,
    })
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::NoSemanticChange);
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
}
