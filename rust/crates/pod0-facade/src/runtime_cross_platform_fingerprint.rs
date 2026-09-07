use pod0_application::{RecallQuery, RecallScope};
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
