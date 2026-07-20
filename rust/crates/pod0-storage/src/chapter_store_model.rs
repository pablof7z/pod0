use pod0_domain::{ChapterArtifact, ChapterArtifactId, CommandId, ContentDigest, StateRevision};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterCommitStorageReceipt {
    pub command_id: CommandId,
    pub artifact_id: ChapterArtifactId,
    pub content_digest: ContentDigest,
    pub integrity_digest: ContentDigest,
    pub command_fingerprint: ContentDigest,
    pub previous_artifact_id: Option<ChapterArtifactId>,
    pub selection_revision: StateRevision,
    pub chapter_count: u32,
    pub ad_span_count: u32,
    pub already_selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedChapterArtifact {
    pub selection_revision: StateRevision,
    pub artifact: ChapterArtifact,
}
