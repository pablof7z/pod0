use pod0_application::{RecallQuery, RecallScope};
use pod0_domain::{PublicationArtifactKind, PublicationIntent};
use sha2::{Digest, Sha256};

pub(super) fn hash_recall_query(hash: &mut Sha256, query: &RecallQuery) {
    hash.update(b"recall-query\0");
    hash.update(query.query_id.into_bytes());
    hash.update(query.text.as_bytes());
    hash.update([0]);
    hash.update(query.limit.to_be_bytes());
    match query.scope {
        RecallScope::Library => hash.update([1]),
        RecallScope::Podcast { podcast_id } => {
            hash.update([2]);
            hash.update(podcast_id.into_bytes());
        }
        RecallScope::Episode { episode_id } => {
            hash.update([3]);
            hash.update(episode_id.into_bytes());
        }
        RecallScope::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

pub(super) fn hash_publication(hash: &mut Sha256, intent: &PublicationIntent) {
    hash.update(b"publish-generated-episode\0");
    hash.update(intent.artifact_id.into_bytes());
    match intent.kind {
        PublicationArtifactKind::GeneratedPodcastEpisode => hash.update([1]),
        PublicationArtifactKind::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.update(intent.expected_author_hex.as_bytes());
    hash.update(intent.semantic_revision.to_be_bytes());
    hash.update(intent.media.public_url.as_bytes());
    hash.update([0]);
    hash.update(intent.media.media_type.as_bytes());
    hash.update(intent.media.byte_count.to_be_bytes());
    hash.update(intent.media.content_digest.into_bytes());
}
