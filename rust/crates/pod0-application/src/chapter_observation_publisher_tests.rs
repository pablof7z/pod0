use pod0_domain::{ChapterArtifactSource, MAX_CHAPTERS, StateRevision};

use crate::chapter_observation_test_support::{digest, publisher, qualified};
use crate::{
    ChapterObservationProjection, ChapterObservationRejection, ChapterProjectionScope,
    MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES, project_chapter_artifact,
    qualify_publisher_chapter_observation,
};

#[test]
fn publisher_json_normalizes_supported_fields_and_projects_inferred_ends() {
    let payload = r#"{
      "version":"1.2.0",
      "unknown":"ignored",
      "chapters":[
        {"startTime":50,"endTime":110,"title":" Late ","url":"https://example.test/late"},
        {"startTime":0,"title":"Early"},
        {"startTime":25.5,"title":" Middle  topic ","img":"https://example.test/art.jpg","toc":false},
        {"startTime":35,"title":"   "}
      ]
    }"#;
    let (artifact, _) = qualified(qualify_publisher_chapter_observation(publisher(payload)));

    assert_eq!(artifact.provenance.source, ChapterArtifactSource::Publisher);
    assert_eq!(artifact.provenance.policy_version, 0);
    assert_eq!(artifact.chapters.len(), 3);
    assert_eq!(artifact.chapters[0].title, "Early");
    assert_eq!(artifact.chapters[1].start_milliseconds, 25_500);
    assert_eq!(artifact.chapters[1].title, "Middle topic");
    assert!(!artifact.chapters[1].include_in_table_of_contents);
    assert_eq!(artifact.chapters[2].end_milliseconds, Some(110_000));

    let projection = project_chapter_artifact(
        &artifact,
        StateRevision::new(1),
        ChapterProjectionScope::Chapters,
        0,
        10,
    );
    assert_eq!(projection.chapters[0].explicit_end_milliseconds, None);
    assert_eq!(
        projection.chapters[0].effective_end_milliseconds,
        Some(25_500)
    );
    assert_eq!(
        projection.chapters[2].effective_end_milliseconds,
        Some(110_000)
    );
}

#[test]
fn publisher_missing_version_and_optional_fields_are_supported() {
    let payload = r#"{"chapters":[{"title":"Start"},{"startTime":10.125,"title":"Next"}]}"#;
    let (artifact, _) = qualified(qualify_publisher_chapter_observation(publisher(payload)));

    assert_eq!(artifact.chapters[0].start_milliseconds, 0);
    assert_eq!(artifact.chapters[1].start_milliseconds, 10_125);
    assert!(
        artifact
            .chapters
            .iter()
            .all(|chapter| chapter.image_url.is_none())
    );
    assert!(
        artifact
            .chapters
            .iter()
            .all(|chapter| chapter.link_url.is_none())
    );
}

#[test]
fn publisher_rejects_invalid_wire_and_provenance_inputs_as_state() {
    let valid = r#"{"chapters":[{"startTime":0,"title":"Start"}]}"#;
    let mut bad_digest = publisher(valid);
    bad_digest.payload_digest = digest(b"different bytes");
    assert_rejected(bad_digest, ChapterObservationRejection::DigestMismatch);

    let mut bad_type = publisher(valid);
    bad_type.content_type = "text/html".into();
    assert_rejected(bad_type, ChapterObservationRejection::InvalidContentType);

    let mut bad_source_url = publisher(valid);
    bad_source_url.resolved_source_url = "javascript:alert(1)".into();
    assert_rejected(bad_source_url, ChapterObservationRejection::InvalidUrl);

    let bad_item_url =
        r#"{"chapters":[{"startTime":0,"title":"Start","url":"data:text/plain,no"}]}"#;
    assert_rejected(
        publisher(bad_item_url),
        ChapterObservationRejection::InvalidUrl,
    );
    assert_rejected(
        publisher("not-json"),
        ChapterObservationRejection::MalformedPayload,
    );
    assert_rejected(
        publisher(r#"{"version":"banana","chapters":[]}"#),
        ChapterObservationRejection::MalformedPayload,
    );
    assert_rejected(
        publisher(r#"{"version":"2.0.0","chapters":[]}"#),
        ChapterObservationRejection::UnsupportedFormat { format_version: 2 },
    );
    assert_rejected(
        publisher(r#"{"chapters":[{"title":"   "}]}"#),
        ChapterObservationRejection::NoUsableChapters,
    );
}

#[test]
fn publisher_enforces_document_and_collection_bounds_before_projection() {
    let mut oversized = publisher("{}");
    oversized.payload = vec![b' '; MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES + 1];
    oversized.payload_digest = digest(&oversized.payload);
    assert_rejected(oversized, ChapterObservationRejection::PayloadTooLarge);

    let chapters = (0..=MAX_CHAPTERS)
        .map(|index| serde_json::json!({"startTime": index, "title": "Chapter"}))
        .collect::<Vec<_>>();
    let payload = serde_json::to_string(&serde_json::json!({"chapters": chapters})).unwrap();
    let mut observation = publisher(&payload);
    observation.duration_milliseconds = None;
    assert_rejected(observation, ChapterObservationRejection::CollectionLimit);
}

#[test]
fn publisher_identity_is_stable_for_incidental_time_and_changes_with_source() {
    let payload = r#"{"chapters":[{"startTime":0,"title":"Start"}]}"#;
    let original = publisher(payload);
    let mut later = original.clone();
    later.generated_at.value += 10_000;
    let (first, first_fingerprint) = qualified(qualify_publisher_chapter_observation(original));
    let (second, second_fingerprint) = qualified(qualify_publisher_chapter_observation(later));
    assert_eq!(first.artifact_id, second.artifact_id);
    assert_eq!(first_fingerprint, second_fingerprint);
    assert_ne!(first.integrity_digest, second.integrity_digest);

    let mut changed_source = publisher(payload);
    changed_source.resolved_source_url = "https://cdn.example.test/chapters.json".into();
    let (changed, changed_fingerprint) =
        qualified(qualify_publisher_chapter_observation(changed_source));
    assert_ne!(first.artifact_id, changed.artifact_id);
    assert_ne!(first_fingerprint, changed_fingerprint);
}

fn assert_rejected(
    observation: crate::PublisherChapterObservation,
    expected: ChapterObservationRejection,
) {
    assert_eq!(
        qualify_publisher_chapter_observation(observation),
        ChapterObservationProjection::Rejected { reason: expected }
    );
}
