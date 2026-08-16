use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision,
};

use crate::{
    DownloadControlActivityInput, DownloadControlMutation, DownloadControlOperation,
    RequestDisposition, RequestRejectionReason, plan_download_control,
};

#[test]
fn download_control_has_typed_transitions_and_rejections() {
    let input = DownloadControlActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id: EpisodeId::from_parts(3, 4),
        current_revision: StateRevision::new(5),
        legacy_replay: false,
        operation: DownloadControlOperation::Cancel,
        rejection: None,
        effect: Some(download_effect(EpisodeId::from_parts(3, 4))),
    };
    let accepted = plan_download_control(input.clone()).unwrap();
    assert_eq!(accepted.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, facts, effects, _, _) = accepted.into_parts();
    assert_eq!(mutation, DownloadControlMutation::Apply);
    assert_eq!(facts.len(), 4);
    assert_eq!(effects.len(), 1);

    let rejected = plan_download_control(DownloadControlActivityInput {
        rejection: Some(RequestRejectionReason::RevisionConflict),
        effect: None,
        ..input
    })
    .unwrap();
    assert_eq!(
        rejected.disposition(),
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        }
    );
    assert_eq!(rejected.into_parts().3.len(), 1);
}

fn download_effect(episode_id: EpisodeId) -> crate::DownloadEffectAuthorization {
    crate::DownloadEffectAuthorization {
        request: crate::DurableDownloadEffectRequest {
            request_id: HostRequestId::from_parts(5, 1),
            command_id: CommandId::from_parts(1, 2),
            cancellation_id: CancellationId::from_parts(5, 2),
            issued_revision: StateRevision::new(5),
            not_before: None,
            deadline_at: None,
            action: crate::DurableDownloadEffectAction::Cancel {
                episode_id,
                intent_id: DownloadIntentId::from_parts(5, 3),
                attempt_id: DownloadAttemptId::from_parts(5, 4),
                external_task_key: None,
            },
        },
    }
}
