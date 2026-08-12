use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ChapterArtifactActivityInput, ChapterArtifactMutation, RequestDisposition,
    RequestRejectionReason, plan_chapter_artifact_commit,
};

#[test]
fn chapter_ingestion_records_adoption_selection_and_exact_rejections() {
    let input = ChapterArtifactActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id: EpisodeId::from_parts(3, 4),
        current_selection_revision: StateRevision::new(5),
        expected_selection_revision: StateRevision::new(5),
        legacy_replay: false,
        artifact_is_valid: true,
        transcript_provenance_is_current: true,
    };
    let accepted = plan_chapter_artifact_commit(input).unwrap();
    assert_eq!(accepted.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, facts, _, _, _) = accepted.into_parts();
    assert_eq!(mutation, ChapterArtifactMutation::Commit);
    assert_eq!(facts.len(), 3);

    for (changed, expected) in [
        (
            ChapterArtifactActivityInput {
                artifact_is_valid: false,
                ..input
            },
            RequestRejectionReason::Invalid,
        ),
        (
            ChapterArtifactActivityInput {
                transcript_provenance_is_current: false,
                ..input
            },
            RequestRejectionReason::RevisionConflict,
        ),
    ] {
        assert_eq!(
            plan_chapter_artifact_commit(changed).unwrap().disposition(),
            RequestDisposition::Rejected { reason: expected }
        );
    }
}
