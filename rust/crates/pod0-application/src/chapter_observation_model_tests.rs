use pod0_domain::{
    AdSpanEvaluation, ChapterAdKind, ChapterArtifactSource, MAX_CHAPTERS, TranscriptVersionId,
};

use crate::chapter_observation_test_support::{digest, model, publisher, qualified};
use crate::{
    ChapterModelObservationMode, ChapterObservationProjection, ChapterObservationRejection,
    MAX_MODEL_CHAPTER_COMPLETION_BYTES, ModelChapterObservation, qualify_model_chapter_observation,
    qualify_publisher_chapter_observation,
};

const GENERATED: &str = r#"{
  "chapters":[
    {"start":30,"title":" Opening ","summary":" First   idea. "},
    {"start":30,"title":"Duplicate"},
    {"start":20,"title":"Backwards"},
    {"start":50,"title":"Second","summary":"   "},
    {"start":80.25,"title":"Third"},
    {"start":110,"title":"Fourth"}
  ],
  "ads":[
    {"start":-2,"end":5,"kind":"preroll"},
    {"start":4,"end":8,"kind":"midroll"},
    {"start_seconds":60,"end_seconds":70,"kind":"unknown"},
    {"start":110,"end":130,"kind":"postroll"}
  ]
}"#;

#[test]
fn generated_output_normalizes_chapters_summaries_and_ads() {
    let (artifact, _) = qualified(qualify_model_chapter_observation(model(
        GENERATED,
        ChapterModelObservationMode::Generate,
    )));

    assert_eq!(artifact.provenance.source, ChapterArtifactSource::Generated);
    assert_eq!(artifact.provenance.provider.as_deref(), Some("openrouter"));
    assert_eq!(
        artifact.provenance.model.as_deref(),
        Some("fixture-model-v1")
    );
    assert!(artifact.provenance.transcript_version_id.is_some());
    assert_eq!(artifact.chapters.len(), 4);
    assert_eq!(artifact.chapters[0].start_milliseconds, 0);
    assert_eq!(artifact.chapters[0].title, "Opening");
    assert_eq!(artifact.chapters[0].summary.as_deref(), Some("First idea."));
    assert_eq!(artifact.chapters[1].start_milliseconds, 50_000);
    assert_eq!(artifact.chapters[1].summary, None);
    assert_eq!(artifact.chapters[2].start_milliseconds, 80_250);

    assert_eq!(artifact.ad_span_evaluation, AdSpanEvaluation::Evaluated);
    assert_eq!(artifact.ad_spans.len(), 3);
    assert_eq!(artifact.ad_spans[0].start_milliseconds, 0);
    assert_eq!(artifact.ad_spans[0].kind, ChapterAdKind::Preroll);
    assert_eq!(artifact.ad_spans[1].kind, ChapterAdKind::Midroll);
    assert_eq!(artifact.ad_spans[2].end_milliseconds, 120_000);
    assert_eq!(artifact.ad_spans[2].kind, ChapterAdKind::Postroll);
}

#[test]
fn enrichment_preserves_publisher_boundaries_and_applies_indexed_summaries() {
    let base_payload = r#"{"chapters":[
      {"startTime":0,"title":"Publisher one","url":"https://example.test/one"},
      {"startTime":60,"title":"Publisher two"}
    ]}"#;
    let (base, _) = qualified(qualify_publisher_chapter_observation(publisher(
        base_payload,
    )));
    let completion = r#"{
      "summaries":[
        {"index":0,"summary":"  Added   context. "},
        {"index":-1,"summary":"Ignored"},
        {"index":7,"summary":"Ignored"},
        {"index":1,"summary":"   "}
      ],
      "ads":[]
    }"#;
    let (artifact, _) = qualified(qualify_model_chapter_observation(model(
        completion,
        ChapterModelObservationMode::Enrich {
            publisher_artifact: base.as_input(),
        },
    )));

    assert_eq!(
        artifact.provenance.source,
        ChapterArtifactSource::PublisherEnriched
    );
    assert_eq!(artifact.chapters[0].start_milliseconds, 0);
    assert_eq!(artifact.chapters[0].title, "Publisher one");
    assert_eq!(
        artifact.chapters[0].link_url.as_deref(),
        Some("https://example.test/one")
    );
    assert_eq!(
        artifact.chapters[0].summary.as_deref(),
        Some("Added context.")
    );
    assert_eq!(artifact.chapters[1].summary, None);
    assert_eq!(artifact.ad_span_evaluation, AdSpanEvaluation::Evaluated);
    assert!(artifact.ad_spans.is_empty());
}

#[test]
fn model_rejects_malformed_version_policy_digest_and_stale_evidence() {
    let mut digest_mismatch = model(GENERATED, ChapterModelObservationMode::Generate);
    digest_mismatch.completion_digest = digest(b"different completion");
    assert_rejected(digest_mismatch, ChapterObservationRejection::DigestMismatch);

    let fenced = "```json\n{}\n```";
    assert_rejected(
        model(fenced, ChapterModelObservationMode::Generate),
        ChapterObservationRejection::MalformedPayload,
    );
    assert_rejected(
        model("not-json", ChapterModelObservationMode::Generate),
        ChapterObservationRejection::MalformedPayload,
    );

    let mut future_format = model(GENERATED, ChapterModelObservationMode::Generate);
    future_format.format_version = 2;
    assert_rejected(
        future_format,
        ChapterObservationRejection::UnsupportedFormat { format_version: 2 },
    );
    let mut future_policy = model(GENERATED, ChapterModelObservationMode::Generate);
    future_policy.policy_version = 2;
    assert_rejected(
        future_policy,
        ChapterObservationRejection::UnsupportedPolicy { policy_version: 2 },
    );

    let mut stale = model(GENERATED, ChapterModelObservationMode::Generate);
    stale.requested_transcript_version_id = TranscriptVersionId::from_parts(50, 1);
    assert_rejected(stale, ChapterObservationRejection::StaleTranscriptEvidence);

    let mut invalid_provider = model(GENERATED, ChapterModelObservationMode::Generate);
    invalid_provider.provider = " openrouter".into();
    assert_rejected(
        invalid_provider,
        ChapterObservationRejection::InvalidProvenance,
    );
}

#[test]
fn model_rejects_out_of_range_base_and_unusable_generated_output() {
    let out_of_range = r#"{"chapters":[
      {"start":0,"title":"A"},{"start":30,"title":"B"},
      {"start":60,"title":"C"},{"start":200,"title":"D"}
    ],"ads":[]}"#;
    assert_rejected(
        model(out_of_range, ChapterModelObservationMode::Generate),
        ChapterObservationRejection::InvalidRange,
    );

    let too_few = r#"{"chapters":[
      {"start":0,"title":"A"},{"start":0,"title":"duplicate"},
      {"start":20,"title":"B"},{"start":10,"title":"backwards"}
    ]}"#;
    assert_rejected(
        model(too_few, ChapterModelObservationMode::Generate),
        ChapterObservationRejection::NoUsableChapters,
    );

    let mut wrong_base = model(GENERATED, ChapterModelObservationMode::Generate);
    let generated = qualified(qualify_model_chapter_observation(wrong_base.clone())).0;
    wrong_base.mode = ChapterModelObservationMode::Enrich {
        publisher_artifact: generated.as_input(),
    };
    assert_rejected(wrong_base, ChapterObservationRejection::InvalidBaseArtifact);
}

#[test]
fn model_enforces_text_and_collection_bounds_before_normalization() {
    let oversized = " ".repeat(MAX_MODEL_CHAPTER_COMPLETION_BYTES + 1);
    assert_rejected(
        model(&oversized, ChapterModelObservationMode::Generate),
        ChapterObservationRejection::PayloadTooLarge,
    );

    let chapters = (0..=MAX_CHAPTERS)
        .map(|index| serde_json::json!({"start": index, "title": "Chapter"}))
        .collect::<Vec<_>>();
    let completion = serde_json::to_string(&serde_json::json!({
        "chapters": chapters,
        "ads": []
    }))
    .unwrap();
    let mut observation = model(&completion, ChapterModelObservationMode::Generate);
    observation.duration_milliseconds = None;
    assert_rejected(observation, ChapterObservationRejection::CollectionLimit);
}

#[test]
fn model_identity_is_deterministic_and_material_provenance_changes_generation() {
    let original = model(GENERATED, ChapterModelObservationMode::Generate);
    let mut later = original.clone();
    later.generated_at.value += 5_000;
    let (first, first_fingerprint) = qualified(qualify_model_chapter_observation(original));
    let (second, second_fingerprint) = qualified(qualify_model_chapter_observation(later));
    assert_eq!(first.artifact_id, second.artifact_id);
    assert_eq!(first_fingerprint, second_fingerprint);
    assert_ne!(first.integrity_digest, second.integrity_digest);

    let mut changed = model(GENERATED, ChapterModelObservationMode::Generate);
    changed.model = "fixture-model-v2".into();
    let (changed, changed_fingerprint) = qualified(qualify_model_chapter_observation(changed));
    assert_ne!(first.artifact_id, changed.artifact_id);
    assert_ne!(first_fingerprint, changed_fingerprint);
}

fn assert_rejected(observation: ModelChapterObservation, expected: ChapterObservationRejection) {
    assert_eq!(
        qualify_model_chapter_observation(observation),
        ChapterObservationProjection::Rejected { reason: expected }
    );
}
