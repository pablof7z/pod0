use super::*;

fn digest(value: u64) -> ContentDigest {
    ContentDigest {
        word_0: value,
        word_1: value + 1,
        word_2: value + 2,
        word_3: value + 3,
    }
}

fn input() -> ChapterArtifactInput {
    ChapterArtifactInput {
        episode_id: EpisodeId::from_parts(11, 98),
        podcast_id: PodcastId::from_parts(12, 98),
        source_revision: "generated:chapter-contract-v1".into(),
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::Generated,
            provider: Some("openrouter".into()),
            model: Some("model-v1".into()),
            policy_version: 1,
            source_payload_digest: digest(1),
            transcript_version_id: Some(TranscriptVersionId::from_parts(13, 98)),
            transcript_content_digest: Some(digest(8)),
            legacy_import: None,
        },
        generated_at: UnixTimestampMilliseconds::new(1_721_322_123_456),
        duration_milliseconds: Some(120_000),
        chapters: vec![
            ChapterInput {
                start_milliseconds: 0,
                end_milliseconds: None,
                title: " Calm   by default ".into(),
                summary: Some("A quiet opening.".into()),
                image_url: Some("https://example.test/first.jpg".into()),
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
            ChapterInput {
                start_milliseconds: 60_000,
                end_milliseconds: Some(120_000),
                title: "Alive on demand".into(),
                summary: None,
                image_url: None,
                link_url: Some("https://example.test/second".into()),
                include_in_table_of_contents: true,
                source_episode_id: Some(EpisodeId::from_parts(99, 1)),
            },
        ],
        ad_span_evaluation: AdSpanEvaluation::Evaluated,
        ad_spans: vec![AdSpanInput {
            start_milliseconds: 45_000,
            end_milliseconds: 50_000,
            kind: ChapterAdKind::Midroll,
        }],
    }
}

#[test]
fn canonical_artifact_is_stable_and_normalizes_semantic_text() {
    let first = ChapterArtifact::seal(input()).expect("artifact");
    let second = ChapterArtifact::seal(input()).expect("artifact");

    assert_eq!(first, second);
    assert_eq!(first.chapters[0].title, "Calm by default");
    assert_ne!(first.chapters[0].chapter_id, first.chapters[1].chapter_id);
    assert_eq!(first.ad_spans.len(), 1);
    assert_eq!(first.verify_integrity(), Ok(()));
}

#[test]
fn material_content_and_provenance_changes_create_new_identities() {
    let baseline = ChapterArtifact::seal(input()).expect("baseline");
    let mut variants = Vec::new();

    let mut changed = input();
    changed.source_revision = "generated:chapter-contract-v2".into();
    variants.push(changed);
    let mut changed = input();
    changed.provenance.model = Some("model-v2".into());
    variants.push(changed);
    let mut changed = input();
    changed.provenance.transcript_version_id = Some(TranscriptVersionId::from_parts(13, 99));
    variants.push(changed);
    let mut changed = input();
    changed.chapters[0].end_milliseconds = Some(59_000);
    variants.push(changed);
    let mut changed = input();
    changed.chapters[0].title = "A changed title".into();
    variants.push(changed);
    let mut changed = input();
    changed.chapters[0].summary = Some("A changed summary.".into());
    variants.push(changed);
    let mut changed = input();
    changed.chapters[1].source_episode_id = Some(EpisodeId::from_parts(99, 2));
    variants.push(changed);
    let mut changed = input();
    changed.ad_spans[0].kind = ChapterAdKind::Preroll;
    variants.push(changed);

    for candidate in variants {
        let changed = ChapterArtifact::seal(candidate).expect("material variant");
        assert_ne!(baseline.artifact_id, changed.artifact_id);
        assert_ne!(baseline.integrity_digest, changed.integrity_digest);
        assert_ne!(
            baseline.chapters[0].chapter_id,
            changed.chapters[0].chapter_id
        );
    }
}

#[test]
fn explicit_empty_ad_evaluation_differs_from_not_evaluated() {
    let mut evaluated = input();
    evaluated.ad_spans.clear();
    let evaluated = ChapterArtifact::seal(evaluated).expect("evaluated empty");
    let mut pending = input();
    pending.ad_span_evaluation = AdSpanEvaluation::NotEvaluated;
    pending.ad_spans.clear();
    let pending = ChapterArtifact::seal(pending).expect("not evaluated");

    assert_ne!(evaluated.content_digest, pending.content_digest);
    assert_ne!(evaluated.artifact_id, pending.artifact_id);
}

#[test]
fn malformed_ranges_collections_and_forward_values_fail_closed() {
    let mut empty = input();
    empty.chapters.clear();
    assert_eq!(
        ChapterArtifact::seal(empty),
        Err(ChapterArtifactError::InvalidChapter)
    );

    let mut overlap = input();
    overlap.chapters[0].end_milliseconds = Some(61_000);
    assert_eq!(
        ChapterArtifact::seal(overlap),
        Err(ChapterArtifactError::ChaptersOverlap)
    );

    let mut out_of_order = input();
    out_of_order.chapters.swap(0, 1);
    assert_eq!(
        ChapterArtifact::seal(out_of_order),
        Err(ChapterArtifactError::ChaptersOutOfOrder)
    );

    let mut beyond_duration = input();
    beyond_duration.chapters[1].end_milliseconds = Some(120_001);
    assert_eq!(
        ChapterArtifact::seal(beyond_duration),
        Err(ChapterArtifactError::InvalidChapter)
    );

    let mut invalid_ad = input();
    invalid_ad.ad_spans[0].end_milliseconds = invalid_ad.ad_spans[0].start_milliseconds;
    assert_eq!(
        ChapterArtifact::seal(invalid_ad),
        Err(ChapterArtifactError::InvalidAdSpan)
    );

    let mut unsupported = input();
    unsupported.provenance.source = ChapterArtifactSource::Unsupported { wire_code: 4_242 };
    assert_eq!(
        ChapterArtifact::seal(unsupported),
        Err(ChapterArtifactError::UnsupportedSource { wire_code: 4_242 })
    );

    let mut unsupported_ad = input();
    unsupported_ad.ad_spans[0].kind = ChapterAdKind::Unsupported { wire_code: 4_243 };
    assert_eq!(
        ChapterArtifact::seal(unsupported_ad),
        Err(ChapterArtifactError::UnsupportedAdKind { wire_code: 4_243 })
    );

    let mut unsupported_evaluation = input();
    unsupported_evaluation.ad_span_evaluation = AdSpanEvaluation::Unsupported { wire_code: 4_244 };
    assert_eq!(
        ChapterArtifact::seal(unsupported_evaluation),
        Err(ChapterArtifactError::UnsupportedAdEvaluation { wire_code: 4_244 })
    );
}

#[test]
fn provenance_collection_and_text_limits_fail_closed() {
    let mut invalid_provenance = input();
    invalid_provenance.provenance.transcript_content_digest = None;
    assert_eq!(
        ChapterArtifact::seal(invalid_provenance),
        Err(ChapterArtifactError::InvalidProvenance)
    );

    let mut too_many = input();
    too_many.chapters = vec![too_many.chapters[0].clone(); MAX_CHAPTERS + 1];
    assert_eq!(
        ChapterArtifact::seal(too_many),
        Err(ChapterArtifactError::TooManyChapters)
    );

    let mut oversized = input();
    oversized.chapters[0].title = "x".repeat(MAX_CHAPTER_TITLE_BYTES + 1);
    assert_eq!(
        ChapterArtifact::seal(oversized),
        Err(ChapterArtifactError::TextLimit)
    );

    let mut unevaluated_with_span = input();
    unevaluated_with_span.ad_span_evaluation = AdSpanEvaluation::NotEvaluated;
    assert_eq!(
        ChapterArtifact::seal(unevaluated_with_span),
        Err(ChapterArtifactError::InvalidAdSpan)
    );
}

#[test]
fn explicit_legacy_provenance_preserves_unknown_generation_evidence() {
    let mut legacy = input();
    legacy.generated_at = UnixTimestampMilliseconds::new(0);
    legacy.provenance.provider = None;
    legacy.provenance.model = None;
    legacy.provenance.transcript_version_id = None;
    legacy.provenance.transcript_content_digest = None;
    legacy.provenance.legacy_import = Some(ChapterLegacyProvenance {
        source: ChapterLegacySource::EpisodeAdjunct,
        original_origin: None,
        generated_at_was_unknown: true,
    });
    let artifact = ChapterArtifact::seal(legacy).expect("legacy artifact");
    assert_eq!(
        artifact
            .provenance
            .legacy_import
            .expect("legacy evidence")
            .source,
        ChapterLegacySource::EpisodeAdjunct
    );

    let mut invalid = input();
    invalid.provenance.legacy_import = Some(ChapterLegacyProvenance {
        source: ChapterLegacySource::EpisodeAdjunct,
        original_origin: None,
        generated_at_was_unknown: true,
    });
    assert_eq!(
        ChapterArtifact::seal(invalid),
        Err(ChapterArtifactError::InvalidProvenance)
    );
}

#[test]
fn integrity_verification_detects_tampering() {
    let mut artifact = ChapterArtifact::seal(input()).expect("artifact");
    artifact.chapters[0].title = "tampered".into();
    assert_eq!(
        artifact.verify_integrity(),
        Err(ChapterArtifactError::IdentityMismatch)
    );
}
