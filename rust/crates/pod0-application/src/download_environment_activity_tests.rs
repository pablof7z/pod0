use pod0_domain::{CommandId, StateRevision};

use crate::{
    DownloadEnvironmentActivityInput, DownloadEnvironmentMutation, RequestDisposition,
    plan_download_environment,
};

#[test]
fn download_environment_observation_is_a_typed_global_transition() {
    let input = DownloadEnvironmentActivityInput {
        command_id: CommandId::from_parts(1, 2),
        current_revision: StateRevision::new(5),
        legacy_replay: false,
    };
    let accepted = plan_download_environment(input).unwrap();
    assert_eq!(accepted.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, facts, _, _, _) = accepted.into_parts();
    assert_eq!(mutation, DownloadEnvironmentMutation::Apply);
    assert_eq!(facts.len(), 2);

    assert_eq!(
        plan_download_environment(DownloadEnvironmentActivityInput {
            legacy_replay: true,
            ..input
        })
        .unwrap()
        .disposition(),
        RequestDisposition::Duplicate
    );
}
