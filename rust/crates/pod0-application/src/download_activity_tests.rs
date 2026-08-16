use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision,
};

use crate::{
    ActivityDomain, ActivityFact, ActivitySubject, DownloadAdmissionActivityInput,
    DownloadDispositionActivityInput, DownloadEffectAuthorization, DownloadIntentOrigin,
    DurableInternalCommandRequest, InternalCommandKind, RequestDisposition,
    plan_download_admission, plan_download_noop,
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
        effect: Some(download_effect(EpisodeId::from_parts(3, 4))),
        origin: DownloadIntentOrigin::User,
    })
    .unwrap();
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
    assert_eq!(effects.len(), 1);
    let crate::DurableEffectExecution::Download { request } = &effects[0].request.execution else {
        panic!("download authorization must retain an exact executable request");
    };
    assert_eq!(request.episode_id(), EpisodeId::from_parts(3, 4));
    assert!(matches!(
        request.action,
        crate::DurableDownloadEffectAction::Start { .. }
    ));
}

fn download_effect(episode_id: EpisodeId) -> DownloadEffectAuthorization {
    DownloadEffectAuthorization {
        request: crate::DurableDownloadEffectRequest {
            request_id: HostRequestId::from_parts(5, 1),
            command_id: CommandId::from_parts(1, 2),
            cancellation_id: CancellationId::from_parts(5, 2),
            issued_revision: StateRevision::new(5),
            not_before: None,
            deadline_at: None,
            action: crate::DurableDownloadEffectAction::Start {
                episode_id,
                intent_id: DownloadIntentId::from_parts(5, 3),
                attempt_id: DownloadAttemptId::from_parts(5, 4),
                input_version: "input".to_owned(),
                enclosure_url: "https://example.test/audio.mp3".to_owned(),
                resume_key: None,
            },
        },
    }
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
        effect: None,
        origin: DownloadIntentOrigin::Playback,
    })
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::NoSemanticChange);
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
}
