use crate::chapter_model_policy_tests::{plan_input, publisher_artifact};
use crate::{ChapterModelPlan, plan_chapter_model_request};

const GENERATION_SYSTEM: &str =
    include_str!("../../../../Fixtures/CoreKnowledge/chapter-model-generation-system-v1.txt");
const GENERATION_USER: &str =
    include_str!("../../../../Fixtures/CoreKnowledge/chapter-model-generation-user-v1.txt");
const ENRICHMENT_SYSTEM: &str =
    include_str!("../../../../Fixtures/CoreKnowledge/chapter-model-enrichment-system-v1.txt");
const ENRICHMENT_USER: &str =
    include_str!("../../../../Fixtures/CoreKnowledge/chapter-model-enrichment-user-v1.txt");

#[test]
fn generated_prompt_matches_the_characterized_ios_golden() {
    let ChapterModelPlan::Ready { request } = plan_chapter_model_request(plan_input(None)) else {
        panic!("generation fixture must be ready")
    };
    assert_eq!(request.system_prompt, fixture(GENERATION_SYSTEM));
    assert_eq!(request.user_prompt, fixture(GENERATION_USER));
}

#[test]
fn enriched_prompt_matches_the_characterized_ios_golden() {
    let ChapterModelPlan::Ready { request } =
        plan_chapter_model_request(plan_input(Some(publisher_artifact())))
    else {
        panic!("enrichment fixture must be ready")
    };
    assert_eq!(request.system_prompt, fixture(ENRICHMENT_SYSTEM));
    assert_eq!(request.user_prompt, fixture(ENRICHMENT_USER));
}

fn fixture(value: &str) -> &str {
    value.strip_suffix('\n').unwrap_or(value)
}
