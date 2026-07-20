use std::collections::BTreeMap;

use pod0_domain::{
    AdSpanEvaluation, AdSpanId, AdSpanInput, ChapterAdKind, ChapterArtifactId,
    ChapterArtifactInput, ChapterArtifactProvenance, ChapterArtifactSource, ChapterId,
    ChapterInput, CommandId, ContentDigest, EpisodeId, PodcastId, StateRevision,
    TranscriptVersionId, UnixTimestampMilliseconds,
};

use crate::{
    ChapterContractProjection, ChapterContractRequest, ChapterProjectionScope,
    FACADE_CONTRACT_VERSION, project_chapter_contract,
};

fn values() -> BTreeMap<&'static str, &'static str> {
    include_str!("../../../../Fixtures/CoreKnowledge/chapter-contract-v1.properties")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('=').expect("valid golden property"))
        .collect()
}

fn number<T: std::str::FromStr>(values: &BTreeMap<&str, &str>, key: &str) -> T {
    values[key]
        .parse()
        .unwrap_or_else(|_| panic!("valid {key}"))
}

fn id(values: &BTreeMap<&str, &str>, prefix: &str) -> (u64, u64) {
    (
        number(values, &format!("{prefix}_high")),
        number(values, &format!("{prefix}_low")),
    )
}

fn digest(values: &BTreeMap<&str, &str>, prefix: &str) -> ContentDigest {
    ContentDigest {
        word_0: number(values, &format!("{prefix}_word_0")),
        word_1: number(values, &format!("{prefix}_word_1")),
        word_2: number(values, &format!("{prefix}_word_2")),
        word_3: number(values, &format!("{prefix}_word_3")),
    }
}

fn optional(values: &BTreeMap<&str, &str>, key: &str) -> Option<String> {
    (values[key] != "none").then(|| values[key].to_owned())
}

fn request(values: &BTreeMap<&str, &str>) -> ChapterContractRequest {
    let (command_high, command_low) = id(values, "command_id");
    let (episode_high, episode_low) = id(values, "episode_id");
    let (podcast_high, podcast_low) = id(values, "podcast_id");
    let (transcript_high, transcript_low) = id(values, "transcript_version_id");
    let chapters = (0..number(values, "chapter_count"))
        .map(|index| {
            let prefix = format!("chapter_{index}");
            let source_episode_id = if values[format!("{prefix}_source_episode").as_str()] == "none"
            {
                None
            } else {
                let (high, low) = id(values, &format!("{prefix}_source_episode_id"));
                Some(EpisodeId::from_parts(high, low))
            };
            ChapterInput {
                start_milliseconds: number(values, &format!("{prefix}_start_milliseconds")),
                end_milliseconds: optional(values, &format!("{prefix}_end_milliseconds"))
                    .map(|value| value.parse().expect("valid end")),
                title: values[format!("{prefix}_title").as_str()].to_owned(),
                summary: optional(values, &format!("{prefix}_summary")),
                image_url: optional(values, &format!("{prefix}_image_url")),
                link_url: optional(values, &format!("{prefix}_link_url")),
                include_in_table_of_contents: number::<bool>(
                    values,
                    &format!("{prefix}_include_in_toc"),
                ),
                source_episode_id,
            }
        })
        .collect();
    let ad_spans = (0..number(values, "ad_span_count"))
        .map(|index| {
            let prefix = format!("ad_span_{index}");
            assert_eq!(values[format!("{prefix}_kind").as_str()], "midroll");
            AdSpanInput {
                start_milliseconds: number(values, &format!("{prefix}_start_milliseconds")),
                end_milliseconds: number(values, &format!("{prefix}_end_milliseconds")),
                kind: ChapterAdKind::Midroll,
            }
        })
        .collect();
    assert_eq!(values["source"], "publisher_enriched");
    assert_eq!(values["ad_span_evaluation"], "evaluated");
    ChapterContractRequest {
        command_id: CommandId::from_parts(command_high, command_low),
        expected_selection_revision: StateRevision::new(number(
            values,
            "expected_selection_revision",
        )),
        artifact: ChapterArtifactInput {
            episode_id: EpisodeId::from_parts(episode_high, episode_low),
            podcast_id: PodcastId::from_parts(podcast_high, podcast_low),
            source_revision: values["source_revision"].to_owned(),
            provenance: ChapterArtifactProvenance {
                source: ChapterArtifactSource::PublisherEnriched,
                provider: Some(values["provider"].to_owned()),
                model: Some(values["model"].to_owned()),
                policy_version: number(values, "policy_version"),
                source_payload_digest: digest(values, "source_payload_digest"),
                transcript_version_id: Some(TranscriptVersionId::from_parts(
                    transcript_high,
                    transcript_low,
                )),
                transcript_content_digest: Some(digest(values, "transcript_content_digest")),
                legacy_import: None,
            },
            generated_at: UnixTimestampMilliseconds::new(number(
                values,
                "generated_at_milliseconds",
            )),
            duration_milliseconds: Some(number(values, "duration_milliseconds")),
            chapters,
            ad_span_evaluation: AdSpanEvaluation::Evaluated,
            ad_spans,
        },
    }
}

#[test]
fn rust_qualifies_the_cross_platform_chapter_fixture() {
    let values = values();
    assert_eq!(number::<u32>(&values, "fixture_version"), 1);
    assert_eq!(
        number::<u32>(&values, "contract_version"),
        FACADE_CONTRACT_VERSION
    );
    assert_eq!(values["unknown_future_field"], "ignored-by-v1-readers");
    let qualified =
        project_chapter_contract(request(&values), ChapterProjectionScope::Chapters, 0, 1);
    let ChapterContractProjection::Qualified { receipt, artifact } = qualified else {
        panic!("valid fixture rejected");
    };
    let all = project_chapter_contract(request(&values), ChapterProjectionScope::Chapters, 0, 10);
    let ChapterContractProjection::Qualified { artifact: all, .. } = all else {
        panic!("valid fixture rejected");
    };
    let ads = project_chapter_contract(request(&values), ChapterProjectionScope::AdSpans, 0, 10);
    let ChapterContractProjection::Qualified { artifact: ads, .. } = ads else {
        panic!("valid fixture rejected");
    };
    let (artifact_high, artifact_low) = id(&values, "expected_artifact_id");
    assert_eq!(
        receipt.artifact_id,
        ChapterArtifactId::from_parts(artifact_high, artifact_low)
    );
    assert_eq!(
        receipt.content_digest,
        digest(&values, "expected_content_digest")
    );
    assert_eq!(
        receipt.integrity_digest,
        digest(&values, "expected_integrity_digest")
    );
    assert_eq!(
        receipt.command_fingerprint,
        digest(&values, "expected_command_fingerprint")
    );
    assert_eq!(
        receipt.selection_revision,
        StateRevision::new(number(&values, "expected_committed_selection_revision"))
    );
    assert_eq!(
        artifact.chapters[0].title,
        values["chapter_0_expected_title"]
    );
    assert_eq!(
        artifact.chapters[0].effective_end_milliseconds,
        Some(number(
            &values,
            "chapter_0_expected_effective_end_milliseconds"
        ))
    );
    let (chapter_high, chapter_low) = id(&values, "expected_chapter_0_id");
    assert_eq!(
        artifact.chapters[0].chapter_id,
        ChapterId::from_parts(chapter_high, chapter_low)
    );
    let (chapter_high, chapter_low) = id(&values, "expected_chapter_1_id");
    assert_eq!(
        all.chapters[1].chapter_id,
        ChapterId::from_parts(chapter_high, chapter_low)
    );
    assert_eq!(
        all.chapters[1].effective_end_milliseconds,
        Some(number(
            &values,
            "chapter_1_expected_effective_end_milliseconds"
        ))
    );
    assert!(artifact.has_more);
    let (ad_high, ad_low) = id(&values, "expected_ad_span_0_id");
    assert_eq!(
        ads.ad_spans[0].ad_span_id,
        AdSpanId::from_parts(ad_high, ad_low)
    );
}
