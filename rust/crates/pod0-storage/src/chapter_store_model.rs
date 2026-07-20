use pod0_domain::{ChapterArtifact, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedChapterArtifact {
    pub selection_revision: StateRevision,
    pub artifact: ChapterArtifact,
}
