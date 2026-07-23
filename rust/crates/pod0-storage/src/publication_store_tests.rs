use std::path::PathBuf;

use pod0_application::PublicationStatusObservation;
use pod0_domain::{
    AgentCommitId, AgentProposalId, AgentTurnId, CommandId, CompletionStatus, ContentDigest,
    ConversationId, DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeId, EpisodeListeningState,
    EpisodeRecord, GeneratedArtifactId, GeneratedAudioArtifactProvenance, PodcastId, PodcastKind,
    PodcastRecord, PublicationArtifactKind, PublicationFactKind, PublicationIntent,
    PublicationMediaEvidence, PublicationRouteId, PublicationStage, TranscriptArtifactStatus,
    UnixTimestampMilliseconds,
};
use tempfile::TempDir;

use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock, PublicationPrepareOutcome,
    PublicationStore,
};

struct Clock;
impl MigrationClock for Clock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

struct Fixture {
    _directory: TempDir,
    path: PathBuf,
    store: PublicationStore,
    intent: PublicationIntent,
    episode: EpisodeRecord,
    podcast: PodcastRecord,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("core.sqlite3");
        CoreStoreMigrator::new(Clock)
            .migrate(
                &path,
                CURRENT_SCHEMA_VERSION,
                &directory.path().join("backup.sqlite3"),
                CommandId::from_parts(1, 1),
            )
            .unwrap();
        let artifact_id = GeneratedArtifactId::from_parts(2, 3);
        let podcast_id = PodcastId::from_parts(4, 5);
        let episode = EpisodeRecord {
            episode_id: EpisodeId::from_parts(6, 7),
            podcast_id,
            publisher_guid: "generated".into(),
            title: "Daily briefing".into(),
            description: "A useful connection.".into(),
            published_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
            duration_milliseconds: Some(12_000),
            enclosure_url: "file:///private/brief.mp3".into(),
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
                turn_id: AgentTurnId::from_parts(10, 11),
                proposal_id: AgentProposalId::from_parts(12, 13),
                commit_id: AgentCommitId::from_parts(14, 15),
                media_content_digest: ContentDigest::from_bytes([16; 32]),
                script_content_digest: ContentDigest::from_bytes([17; 32]),
                media_byte_count: 2_048,
                voice_id: Some("calm".into()),
                model_reference: "test/model".into(),
                committed_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
            }),
        };
        let podcast = PodcastRecord {
            podcast_id,
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
        let intent = PublicationIntent {
            artifact_id,
            kind: PublicationArtifactKind::GeneratedPodcastEpisode,
            expected_author_hex: "ab".repeat(32),
            semantic_revision: 1,
            media: PublicationMediaEvidence {
                public_url: "https://media.example/brief.mp3".into(),
                media_type: "audio/mpeg".into(),
                byte_count: 2_048,
                content_digest: ContentDigest::from_bytes([16; 32]),
            },
        };
        Self {
            store: PublicationStore::open(&path).unwrap(),
            _directory: directory,
            path,
            intent,
            episode,
            podcast,
        }
    }

    fn prepare(&self) -> PublicationPrepareOutcome {
        self.store
            .prepare_generated_episode(
                CommandId::from_parts(20, 21),
                &"cd".repeat(32),
                &self.intent,
                &self.episode,
                &self.podcast,
                UnixTimestampMilliseconds::new(1_800_000_001_000),
            )
            .unwrap()
    }
}

#[test]
fn correlation_is_durable_before_nmp_acceptance_and_command_replay_is_exact() {
    let fixture = Fixture::new();
    let first = fixture.prepare();
    assert!(matches!(first, PublicationPrepareOutcome::Applied(_)));
    let publication = first.record();
    assert!(publication.receipt_id.is_none());
    assert_eq!(publication.correlation_token.len(), 44);
    assert_eq!(publication.stage, PublicationStage::Prepared);
    assert!(matches!(
        fixture.prepare(),
        PublicationPrepareOutcome::Duplicate(_)
    ));

    let reopened = PublicationStore::open(&fixture.path).unwrap();
    assert_eq!(
        reopened
            .publication(publication.publication_id)
            .unwrap()
            .unwrap(),
        *publication
    );
}

#[test]
fn receipt_and_exact_status_facts_survive_restart_without_collapsing_mixed_evidence() {
    let fixture = Fixture::new();
    let publication_id = fixture.prepare().record().publication_id;
    fixture
        .store
        .record_receipt(
            publication_id,
            u64::MAX - 2,
            UnixTimestampMilliseconds::new(1_800_000_002_000),
        )
        .unwrap();
    let ack = PublicationStatusObservation {
        kind: PublicationFactKind::Acknowledged,
        route_id: Some(PublicationRouteId::from_parts(1, 2)),
        attempt: None,
        event_id_hex: Some("ef".repeat(32)),
        observed_at: None,
        detail: None,
    };
    let acknowledged = fixture
        .store
        .observe(
            publication_id,
            &ack,
            UnixTimestampMilliseconds::new(1_800_000_003_000),
        )
        .unwrap();
    assert_eq!(acknowledged.stage, PublicationStage::Acknowledged);
    let duplicate = fixture
        .store
        .observe(
            publication_id,
            &ack,
            UnixTimestampMilliseconds::new(1_800_000_004_000),
        )
        .unwrap();
    assert_eq!(duplicate.revision, acknowledged.revision);

    let mixed = fixture
        .store
        .observe(
            publication_id,
            &PublicationStatusObservation {
                kind: PublicationFactKind::Rejected,
                route_id: Some(PublicationRouteId::from_parts(3, 4)),
                attempt: None,
                event_id_hex: None,
                observed_at: None,
                detail: Some("policy".into()),
            },
            UnixTimestampMilliseconds::new(1_800_000_005_000),
        )
        .unwrap();
    assert_eq!(mixed.stage, PublicationStage::EvidenceMixed);
    assert_eq!(mixed.facts.len(), 2);
    assert_eq!(mixed.receipt_id, Some(u64::MAX - 2));
    assert_eq!(
        PublicationStore::open(&fixture.path)
            .unwrap()
            .publication(publication_id)
            .unwrap()
            .unwrap(),
        mixed
    );
}

#[test]
fn not_found_and_unreadable_reattachment_outcomes_remain_distinct() {
    let fixture = Fixture::new();
    let publication_id = fixture.prepare().record().publication_id;
    for kind in [
        PublicationFactKind::ReattachmentNotFound,
        PublicationFactKind::ReattachmentUnreadable,
    ] {
        fixture
            .store
            .observe(
                publication_id,
                &PublicationStatusObservation {
                    kind,
                    route_id: None,
                    attempt: None,
                    event_id_hex: None,
                    observed_at: None,
                    detail: None,
                },
                UnixTimestampMilliseconds::new(1_800_000_006_000),
            )
            .unwrap();
    }
    let record = fixture.store.publication(publication_id).unwrap().unwrap();
    assert_eq!(record.stage, PublicationStage::Blocked);
    assert_ne!(record.facts[0].kind, record.facts[1].kind);
}
