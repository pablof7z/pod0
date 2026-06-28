use serde::{Deserialize, Serialize};

use crate::store::notes::NoteTarget;
use crate::store::notes::UserNote;

/// One local note row projected from Rust-owned `NotesState`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct NoteSummary {
    pub id: String,
    pub text: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NoteTarget>,
    pub created_at: i64,
    pub deleted: bool,
    pub author: String,
}

impl From<UserNote> for NoteSummary {
    fn from(note: UserNote) -> Self {
        Self {
            id: note.id,
            text: note.text,
            kind: note.kind,
            target: note.target,
            created_at: note.created_at,
            deleted: note.deleted,
            author: note.author,
        }
    }
}
