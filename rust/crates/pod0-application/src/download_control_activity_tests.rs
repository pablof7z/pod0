use pod0_domain::{CommandId, EpisodeId, StateRevision};

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
    };
    let accepted = plan_download_control(input).unwrap();
    assert_eq!(accepted.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, facts, _, _, _) = accepted.into_parts();
    assert_eq!(mutation, DownloadControlMutation::Apply);
    assert_eq!(facts.len(), 3);

    let rejected = plan_download_control(DownloadControlActivityInput {
        rejection: Some(RequestRejectionReason::RevisionConflict),
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
