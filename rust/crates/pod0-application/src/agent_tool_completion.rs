#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentToolCompletion {
    NoteCreated { note_id: NoteId },
    MemoryRecorded { memory_id: MemoryId },
    ClipCreated { clip_id: ClipId },
    CategoryChanged { category_id: pod0_domain::CategoryId },
    Failed { code: u32 },
}
