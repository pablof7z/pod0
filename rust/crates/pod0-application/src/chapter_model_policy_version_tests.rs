use pod0_domain::{ChapterArtifactSource, ContentDigest};

use crate::{
    ChapterModelDesiredStateInput, ChapterModelDesiredStatePlan, plan_chapter_model_desired_state,
};

#[test]
fn desired_state_preserves_the_legacy_version_algorithm_and_policy_changes() {
    let digest = ContentDigest::from_bytes([5; 32]);
    let input = ChapterModelDesiredStateInput {
        transcript_content_digest: digest,
        configured_model: "openai/gpt-4o-mini".into(),
        selected_chapter_source: Some(ChapterArtifactSource::Generated),
    };
    let ChapterModelDesiredStatePlan::Compile { input_version } =
        plan_chapter_model_desired_state(input)
    else {
        panic!("generated artifacts remain derivable")
    };
    assert_eq!(input_version.len(), 64);
    assert_eq!(
        input_version,
        crate::chapter_model_policy_source::input_version(
            digest,
            "openai/gpt-4o-mini",
            "chapter-prompt-v1"
        )
    );
    assert_ne!(
        input_version,
        crate::chapter_model_policy_source::input_version(
            digest,
            "openai/gpt-4o-mini",
            "chapter-prompt-v2"
        )
    );
}
