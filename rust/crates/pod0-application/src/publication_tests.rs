use super::*;
use pod0_domain::{
    CompletionStatus, ContentDigest, ConversationId, DownloadArtifactStatus, EpisodeFeedMetadata,
    EpisodeListeningState, GeneratedAudioArtifactProvenance, PodcastId, PodcastKind,
    TranscriptArtifactStatus,
};

fn fixture() -> (PublicationRecord, EpisodeRecord, PodcastRecord) {
    let artifact_id = GeneratedArtifactId::from_parts(1, 2);
    let intent = PublicationIntent {
        artifact_id,
        kind: PublicationArtifactKind::GeneratedPodcastEpisode,
        expected_author_hex: "ab".repeat(32),
        semantic_revision: 1,
        media: PublicationMediaEvidence {
            public_url: "https://media.example/episode.mp3".into(),
            media_type: "audio/mpeg".into(),
            byte_count: 42,
            content_digest: ContentDigest::from_bytes([3; 32]),
        },
    };
    let episode = EpisodeRecord {
        episode_id: pod0_domain::EpisodeId::from_parts(4, 5),
        podcast_id: PodcastId::from_parts(6, 7),
        publisher_guid: "generated".into(),
        title: "A calm briefing".into(),
        description: "Connected ideas.".into(),
        published_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        duration_milliseconds: Some(61_000),
        enclosure_url: "file:///brief.mp3".into(),
        enclosure_mime_type: Some("audio/mpeg".into()),
        image_url: None,
        feed_metadata: EpisodeFeedMetadata::default(),
        listening: EpisodeListeningState {
            resume_position_milliseconds: 0,
            completion: CompletionStatus::InProgress,
        },
        is_starred: false,
        download: DownloadArtifactStatus::Unavailable,
        transcript: TranscriptArtifactStatus::Unavailable,
        generated_audio: Some(GeneratedAudioArtifactProvenance {
            artifact_id,
            conversation_id: ConversationId::from_parts(8, 9),
            turn_id: pod0_domain::AgentTurnId::from_parts(10, 11),
            proposal_id: pod0_domain::AgentProposalId::from_parts(12, 13),
            commit_id: pod0_domain::AgentCommitId::from_parts(14, 15),
            media_content_digest: ContentDigest::from_bytes([3; 32]),
            script_content_digest: ContentDigest::from_bytes([4; 32]),
            media_byte_count: 42,
            voice_id: None,
            model_reference: "model".into(),
            committed_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        }),
    };
    let podcast = PodcastRecord {
        podcast_id: episode.podcast_id,
        kind: PodcastKind::Synthetic,
        feed_identity: None,
        title: "Agent Generated".into(),
        author: "Pod0".into(),
        image_url: None,
        description: String::new(),
        language: None,
        categories: Vec::new(),
        discovered_at: episode.published_at,
        title_is_placeholder: false,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    };
    (
        initial_publication_record(&intent, &episode, episode.published_at),
        episode,
        podcast,
    )
}

#[test]
fn composition_is_deterministic_and_keeps_product_nouns_out_of_nmp() {
    let (record, episode, podcast) = fixture();
    let first = compose_generated_episode_publication(&record, &episode, &podcast).unwrap();
    let second = compose_generated_episode_publication(&record, &episode, &podcast).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.kind, 30_075);
    assert!(first.tags.iter().any(|tag| tag[0] == "imeta"));
    assert!(first.tags.iter().any(|tag| tag[0] == "a"));
    assert_eq!(first.correlation_token.len(), 44);
}

#[test]
fn publication_requires_public_https_evidence_matching_the_artifact() {
    let (mut record, episode, podcast) = fixture();
    record.media.public_url = "file:///brief.mp3".into();
    assert_eq!(
        compose_generated_episode_publication(&record, &episode, &podcast),
        Err(PublicationValidationError::InvalidMediaUrl)
    );
    let (mut record, episode, podcast) = fixture();
    record.media.byte_count += 1;
    assert_eq!(
        compose_generated_episode_publication(&record, &episode, &podcast),
        Err(PublicationValidationError::ArtifactMismatch)
    );
}
