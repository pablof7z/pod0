use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    RequestDisposition, TranscriptArtifactActivityInput, TranscriptArtifactMutation,
    plan_transcript_artifact_commit,
};

#[test]
fn transcript_ingestion_records_adoption_and_selection_or_exact_rejection() {
    let input = TranscriptArtifactActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id: EpisodeId::from_parts(3, 4),
        current_selection_revision: StateRevision::new(5),
        expected_selection_revision: StateRevision::new(5),
        legacy_replay: false,
        artifact_is_valid: true,
    };
    let accepted = plan_transcript_artifact_commit(input).unwrap();
    assert_eq!(accepted.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, facts, _, _, _) = accepted.into_parts();
    assert_eq!(mutation, TranscriptArtifactMutation::Commit);
    assert_eq!(facts.len(), 3);

    let rejected = plan_transcript_artifact_commit(TranscriptArtifactActivityInput {
        expected_selection_revision: StateRevision::new(4),
        ..input
    })
    .unwrap();
    assert!(matches!(
        rejected.disposition(),
        RequestDisposition::Rejected { .. }
    ));
    assert_eq!(rejected.into_parts().3.len(), 1);
}
