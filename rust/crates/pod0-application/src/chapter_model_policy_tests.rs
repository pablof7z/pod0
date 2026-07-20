use pod0_domain::{
    AdSpanEvaluation, ChapterArtifactInput, ChapterArtifactProvenance, ChapterArtifactSource,
    ChapterInput, ContentDigest, EpisodeId, PodcastId, StateRevision, TranscriptVersionId,
    UnixTimestampMilliseconds,
};

use crate::*;

#[test]
fn generated_request_is_deterministic_typed_and_bounded() {
    let input = plan_input(None);
    let first = plan_chapter_model_request(input.clone());
    let second = plan_chapter_model_request(input);
    assert_eq!(first, second);
    let ChapterModelPlan::Ready { request } = first else {
        panic!("generation must be ready")
    };
    assert_eq!(request.provider, "openrouter");
    assert_eq!(request.model, "openai/gpt-4o-mini");
    assert_eq!(request.format_version, 1);
    assert_eq!(request.policy_version, 1);
    assert_eq!(
        request.response_format,
        ChapterModelResponseFormat::JsonObject
    );
    assert_eq!(request.maximum_completion_bytes, 1_048_576);
    assert_eq!(request.duration_milliseconds, Some(125_750));
    assert_eq!(
        request.expected_artifact_source,
        ChapterArtifactSource::Generated
    );
    assert_eq!(
        request.expected_chapter_selection_revision,
        StateRevision::INITIAL
    );
    assert!(matches!(
        request.mode,
        ChapterModelObservationMode::Generate
    ));
    assert!(request.system_prompt.len() + request.user_prompt.len() <= 256 * 1_024);
}

#[test]
fn publisher_and_publisher_enriched_artifacts_choose_enrichment() {
    for source in [
        ChapterArtifactSource::Publisher,
        ChapterArtifactSource::PublisherEnriched,
    ] {
        let mut artifact = publisher_artifact();
        artifact.provenance.source = source;
        if source == ChapterArtifactSource::PublisherEnriched {
            artifact.provenance.provider = Some("openrouter".into());
            artifact.provenance.model = Some("openai/gpt-4o-mini".into());
            artifact.provenance.policy_version = 1;
            artifact.provenance.transcript_version_id = Some(version());
            artifact.provenance.transcript_content_digest = Some(digest());
        }
        let ChapterModelPlan::Ready { request } =
            plan_chapter_model_request(plan_input(Some(artifact)))
        else {
            panic!("publisher artifact must enrich")
        };
        assert_eq!(
            request.expected_artifact_source,
            ChapterArtifactSource::PublisherEnriched
        );
        assert!(matches!(
            request.mode,
            ChapterModelObservationMode::Enrich { .. }
        ));
        assert!(request.user_prompt.contains("use these exact indices"));
    }
}

#[test]
fn agent_composed_is_preserved_and_unsupported_is_rejected() {
    let desired = plan_chapter_model_desired_state(ChapterModelDesiredStateInput {
        transcript_content_digest: digest(),
        configured_model: "openai/gpt-4o-mini".into(),
        selected_chapter_source: Some(ChapterArtifactSource::AgentComposed),
    });
    assert_eq!(desired, ChapterModelDesiredStatePlan::PreserveAgentComposed);

    let mut agent = publisher_artifact();
    agent.provenance.source = ChapterArtifactSource::AgentComposed;
    assert_eq!(
        plan_chapter_model_request(plan_input(Some(agent))),
        ChapterModelPlan::PreserveAgentComposed
    );
    let mut unsupported = publisher_artifact();
    unsupported.provenance.source = ChapterArtifactSource::Unsupported { wire_code: 77 };
    assert_eq!(
        plan_chapter_model_request(plan_input(Some(unsupported))),
        ChapterModelPlan::UnsupportedArtifact
    );
}

#[test]
fn transcript_and_configuration_failures_are_state_shaped() {
    let mut missing = plan_input(None);
    missing.selected_transcript = None;
    assert_eq!(
        plan_chapter_model_request(missing),
        ChapterModelPlan::TranscriptUnavailable
    );

    let mut stale = plan_input(None);
    stale
        .selected_transcript
        .as_mut()
        .unwrap()
        .transcript_version_id = TranscriptVersionId::from_parts(9, 9);
    assert_eq!(
        plan_chapter_model_request(stale),
        ChapterModelPlan::StaleTranscript
    );

    let mut empty = plan_input(None);
    empty.selected_transcript.as_mut().unwrap().segments.clear();
    assert_eq!(
        plan_chapter_model_request(empty),
        ChapterModelPlan::EmptyTranscript
    );

    let mut invalid = plan_input(None);
    invalid.configured_model = "   ".into();
    assert_eq!(
        plan_chapter_model_request(invalid),
        ChapterModelPlan::InvalidConfiguration
    );

    let mut oversized = plan_input(None);
    oversized.selected_transcript.as_mut().unwrap().segments[0].text =
        "x".repeat(MAX_CHAPTER_MODEL_TRANSCRIPT_INPUT_BYTES + 1);
    assert_eq!(
        plan_chapter_model_request(oversized),
        ChapterModelPlan::InputTooLarge
    );
}

#[test]
fn invalid_episode_and_publisher_identity_are_rejected() {
    let mut invalid_time = plan_input(None);
    invalid_time.episode.duration_seconds = Some(f64::NAN);
    assert_eq!(
        plan_chapter_model_request(invalid_time),
        ChapterModelPlan::InvalidInput
    );

    let mut artifact = publisher_artifact();
    artifact.episode_id = EpisodeId::from_parts(99, 99);
    assert_eq!(
        plan_chapter_model_request(plan_input(Some(artifact))),
        ChapterModelPlan::UnsupportedArtifact
    );
}

#[test]
fn desired_state_preserves_the_legacy_version_algorithm_and_policy_changes() {
    let input = ChapterModelDesiredStateInput {
        transcript_content_digest: digest(),
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
            digest(),
            "openai/gpt-4o-mini",
            "chapter-prompt-v1"
        )
    );
    assert_ne!(
        input_version,
        crate::chapter_model_policy_source::input_version(
            digest(),
            "openai/gpt-4o-mini",
            "chapter-prompt-v2"
        )
    );
}

#[test]
fn transcript_prompt_limit_counts_graphemes_without_splitting_clusters() {
    use unicode_segmentation::UnicodeSegmentation as _;

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

pub(crate) fn plan_input(
    selected_chapter_artifact: Option<ChapterArtifactInput>,
) -> ChapterModelPlanInput {
    ChapterModelPlanInput {
        episode: ChapterModelEpisodeInput {
            episode_id: EpisodeId::from_parts(1, 2),
            podcast_id: PodcastId::from_parts(3, 4),
            title: "A Calm Test".into(),
            description: "Ignored by prompt v1".into(),
            duration_seconds: Some(125.75),
        },
        requested_transcript_version_id: version(),
        requested_transcript_content_digest: digest(),
        selected_transcript: Some(ChapterModelTranscriptInput {
            transcript_version_id: version(),
            transcript_content_digest: digest(),
            segments: vec![
                ChapterModelTranscriptSegmentInput {
                    start_seconds: 0.4,
                    text: " Opening thought ".into(),
                },
                ChapterModelTranscriptSegmentInput {
                    start_seconds: 12.5,
                    text: "Second idea".into(),
                },
            ],
        }),
        selected_chapter_artifact,
        expected_chapter_selection_revision: StateRevision::INITIAL,
        configured_model: "openai/gpt-4o-mini".into(),
    }
}

pub(crate) fn publisher_artifact() -> ChapterArtifactInput {
    ChapterArtifactInput {
        episode_id: EpisodeId::from_parts(1, 2),
        podcast_id: PodcastId::from_parts(3, 4),
        source_revision: "publisher-v1".into(),
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::Publisher,
            provider: None,
            model: None,
            policy_version: 0,
            source_payload_digest: ContentDigest::from_bytes([7; 32]),
            transcript_version_id: None,
            transcript_content_digest: None,
            legacy_import: None,
        },
        generated_at: UnixTimestampMilliseconds::new(1_700_000_000_000),
        duration_milliseconds: Some(125_750),
        chapters: vec![chapter(0, "Opening"), chapter(60_000, "Deeper work")],
        ad_span_evaluation: AdSpanEvaluation::NotEvaluated,
        ad_spans: vec![],
    }
}

fn chapter(start_milliseconds: u64, title: &str) -> ChapterInput {
    ChapterInput {
        start_milliseconds,
        end_milliseconds: None,
        title: title.into(),
        summary: None,
        image_url: None,
        link_url: None,
        include_in_table_of_contents: true,
        source_episode_id: None,
    }
}

fn digest() -> ContentDigest {
    ContentDigest::from_bytes([5; 32])
}

fn version() -> TranscriptVersionId {
    TranscriptVersionId::from_parts(5, 6)
}
