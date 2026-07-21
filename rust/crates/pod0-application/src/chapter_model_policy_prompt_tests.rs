use unicode_segmentation::UnicodeSegmentation as _;

use crate::chapter_model_policy_tests::plan_input;
use crate::{
    ChapterModelPlan, MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS, plan_chapter_model_request,
};

#[test]
fn transcript_prompt_limit_counts_graphemes_without_splitting_clusters() {
    let cluster = "e\u{301}";
    let mut input = plan_input(None);
    input.selected_transcript.as_mut().unwrap().segments[0].text =
        cluster.repeat(MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS + 1);
    input
        .selected_transcript
        .as_mut()
        .unwrap()
        .segments
        .truncate(1);
    let ChapterModelPlan::Ready { request } = plan_chapter_model_request(input) else {
        panic!("bounded grapheme transcript must remain valid")
    };
    let body = request
        .user_prompt
        .split("Transcript (timestamped):\n")
        .nth(1)
        .unwrap();
    let text = body.strip_prefix("[0s] ").unwrap();
    assert_eq!(
        text.graphemes(true).count(),
        MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS - 5
    );
    assert!(text.graphemes(true).all(|value| value == cluster));
}
