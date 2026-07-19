use pod0_domain::{
    AdSpanEvaluation, AdSpanInput, ChapterAdKind, ChapterArtifactInput, ChapterArtifactProvenance,
    ChapterArtifactSource, ChapterInput, CommandId, ContentDigest, EpisodeId, PodcastId,
    StateRevision, UnixTimestampMilliseconds,
};

use crate::{
    ChapterContractProjection, ChapterContractRejection, ChapterContractRequest,
    ChapterProjectionScope, project_chapter_contract,
};

fn input() -> ChapterArtifactInput {
    ChapterArtifactInput {
        episode_id: EpisodeId::from_parts(1, 2),
        podcast_id: PodcastId::from_parts(3, 4),
        source_revision: "publisher:fixture".into(),
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::Publisher,
            provider: Some("podcasting-2.0".into()),
            model: None,
            policy_version: 0,
            source_payload_digest: ContentDigest::default(),
            transcript_version_id: None,
            transcript_content_digest: None,
        },
        generated_at: UnixTimestampMilliseconds::new(1_700_000_000_000),
        duration_milliseconds: Some(30_000),
        chapters: vec![
            ChapterInput {
                start_milliseconds: 0,
                end_milliseconds: None,
                title: "First".into(),
                summary: None,
                image_url: None,
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
            ChapterInput {
                start_milliseconds: 10_000,
                end_milliseconds: None,
                title: "Second".into(),
                summary: None,
                image_url: None,
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
            ChapterInput {
                start_milliseconds: 20_000,
                end_milliseconds: Some(30_000),
                title: "Third".into(),
                summary: None,
                image_url: None,
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
        ],
        ad_span_evaluation: AdSpanEvaluation::Evaluated,
        ad_spans: vec![AdSpanInput {
            start_milliseconds: 12_000,
            end_milliseconds: 13_000,
            kind: ChapterAdKind::Midroll,
        }],
    }
}

fn request() -> ChapterContractRequest {
    ChapterContractRequest {
        command_id: CommandId::from_parts(5, 6),
        expected_selection_revision: StateRevision::new(7),
        artifact: input(),
    }
}

#[test]
fn projection_is_bounded_and_infers_effective_ends() {
    let qualified = project_chapter_contract(request(), ChapterProjectionScope::Chapters, 0, 1);
    let ChapterContractProjection::Qualified { receipt, artifact } = qualified else {
        panic!("valid artifact rejected");
    };

    assert_eq!(receipt.selection_revision, StateRevision::new(8));
    assert_eq!(receipt.chapter_count, 3);
    assert_eq!(receipt.ad_span_count, 1);
    assert_eq!(artifact.chapters.len(), 1);
    assert_eq!(
        artifact.chapters[0].effective_end_milliseconds,
        Some(10_000)
    );
    assert!(artifact.has_more);
}

#[test]
fn exact_lookup_and_empty_pages_remain_bounded() {
    let all = project_chapter_contract(request(), ChapterProjectionScope::Chapters, 0, 10);
    let ChapterContractProjection::Qualified { artifact, .. } = all else {
        panic!("valid artifact rejected");
    };
    let second_id = artifact.chapters[1].chapter_id;
    let exact = project_chapter_contract(
        request(),
        ChapterProjectionScope::Chapter {
            chapter_id: second_id,
        },
        0,
        1,
    );
    let ChapterContractProjection::Qualified { artifact, .. } = exact else {
        panic!("valid artifact rejected");
    };
    assert_eq!(artifact.chapters[0].title, "Second");
    assert_eq!(
        artifact.chapters[0].effective_end_milliseconds,
        Some(20_000)
    );
}

#[test]
fn invalid_and_future_inputs_are_state_shaped_rejections() {
    let mut malformed = request();
    malformed.artifact.chapters[1].start_milliseconds = 0;
    assert_eq!(
        project_chapter_contract(malformed, ChapterProjectionScope::Summary, 0, 1),
        ChapterContractProjection::Rejected {
            reason: ChapterContractRejection::InvalidChapter
        }
    );

    let mut future = request();
    future.artifact.provenance.source = ChapterArtifactSource::Unsupported { wire_code: 9_001 };
    assert_eq!(
        project_chapter_contract(future, ChapterProjectionScope::Summary, 0, 1),
        ChapterContractProjection::Rejected {
            reason: ChapterContractRejection::Unsupported { wire_code: 9_001 }
        }
    );
}

#[test]
fn revision_overflow_is_rejected_without_panicking() {
    let mut overflow = request();
    overflow.expected_selection_revision = StateRevision::new(u64::MAX);
    assert_eq!(
        project_chapter_contract(overflow, ChapterProjectionScope::Summary, 0, 1),
        ChapterContractProjection::Rejected {
            reason: ChapterContractRejection::RevisionExhausted
        }
    );
}
