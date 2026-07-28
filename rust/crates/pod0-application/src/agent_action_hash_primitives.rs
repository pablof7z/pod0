use pod0_domain::LibraryItemId;
use sha2::{Digest as _, Sha256};

use crate::{ALL_AGENT_TOOL_NAMES, AgentToolName, QueuePlacement, RecallScope};

pub(crate) fn fields(hasher: &mut Sha256, tag: u32, write: impl FnOnce(&mut Sha256)) {
    hash_tag(hasher, tag);
    write(hasher);
}

pub(crate) fn queue_code(value: QueuePlacement) -> u32 {
    match value {
        QueuePlacement::Back => 1,
        QueuePlacement::Next => 2,
        QueuePlacement::Unsupported { wire_code } => wire_code,
    }
}

pub(crate) fn hash_tag(hasher: &mut Sha256, value: u32) {
    hasher.update(value.to_be_bytes());
}

pub(crate) fn hash_tool(hasher: &mut Sha256, tool: AgentToolName) {
    let name = ALL_AGENT_TOOL_NAMES
        .iter()
        .find_map(|(name, candidate)| (*candidate == tool).then_some(*name))
        .expect("every typed tool has a stable wire name");
    hash_text(hasher, name);
}

pub(crate) fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update(
        u64::try_from(value.len())
            .expect("bounded action text length fits u64")
            .to_be_bytes(),
    );
    hasher.update(value.as_bytes());
}

pub(crate) fn hash_optional_text(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hash_text(hasher, value);
        }
        None => hasher.update([0]),
    }
}

pub(crate) fn hash_optional_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

pub(crate) fn hash_optional_id(hasher: &mut Sha256, value: Option<[u8; 16]>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value);
        }
        None => hasher.update([0]),
    }
}

pub(crate) fn hash_recall_scope(hasher: &mut Sha256, scope: RecallScope) {
    match scope {
        RecallScope::Library => hash_tag(hasher, 1),
        RecallScope::Podcast { podcast_id } => {
            hash_tag(hasher, 2);
            hasher.update(podcast_id.into_bytes());
        }
        RecallScope::Episode { episode_id } => {
            hash_tag(hasher, 3);
            hasher.update(episode_id.into_bytes());
        }
        RecallScope::Unsupported { wire_code } => {
            hash_tag(hasher, 4);
            hash_tag(hasher, wire_code);
        }
    }
}

/// Length-prefixed so `[a, b]` and `[ab]` cannot collide, and order is
/// preserved: two calls differing only in item order are distinct proposals.
pub(crate) fn hash_item_ids(hasher: &mut Sha256, items: &[LibraryItemId]) {
    hasher.update(
        u64::try_from(items.len())
            .expect("bounded item count fits u64")
            .to_be_bytes(),
    );
    for item in items {
        hasher.update(item.into_bytes());
    }
}
