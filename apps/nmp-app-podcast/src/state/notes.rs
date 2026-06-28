//! Local notes substate.
//!
//! Owns user/agent local notes in Rust and writes through to `notes.json`.
//! Native shells should eventually dispatch note mutations here and render the
//! `PodcastUpdate.notes` projection instead of persisting parallel note arrays.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::notes::{self, NoteTarget, UserNote};
use crate::store::PodcastStore;

/// Rust-owned local notes state.
pub struct NotesState {
    notes: Slot<Vec<UserNote>, Session>,
    infra: Infra,
    store: Arc<Mutex<PodcastStore>>,
}

impl NotesState {
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        let seed = Self::data_dir_from_store(&store)
            .map(|dir| notes::load_notes(&dir))
            .unwrap_or_default();
        Self {
            notes: Slot::new(seed),
            infra,
            store,
        }
    }

    pub fn notes_snapshot(&self) -> Vec<UserNote> {
        self.notes
            .lock()
            .ok()
            .map(|n| n.clone())
            .unwrap_or_default()
    }

    pub fn add_note(&self, note: UserNote) -> bool {
        let changed = if let Ok(mut notes) = self.notes.lock() {
            if notes.iter().any(|existing| existing.id == note.id) {
                false
            } else {
                notes.push(note);
                true
            }
        } else {
            false
        };
        self.persist_and_bump(changed);
        changed
    }

    pub fn update_note(
        &self,
        id: &str,
        text: Option<String>,
        kind: Option<String>,
        target: Option<Option<NoteTarget>>,
    ) -> bool {
        let changed = if let Ok(mut notes) = self.notes.lock() {
            if let Some(note) = notes.iter_mut().find(|note| note.id == id) {
                let mut changed = false;
                if let Some(text) = text {
                    changed |= note.text != text;
                    note.text = text;
                }
                if let Some(kind) = kind {
                    changed |= note.kind != kind;
                    note.kind = kind;
                }
                if let Some(target) = target {
                    changed |= note.target != target;
                    note.target = target;
                }
                changed
            } else {
                false
            }
        } else {
            false
        };
        self.persist_and_bump(changed);
        changed
    }

    pub fn set_deleted(&self, id: &str, deleted: bool) -> bool {
        let changed = if let Ok(mut notes) = self.notes.lock() {
            if let Some(note) = notes.iter_mut().find(|note| note.id == id) {
                if note.deleted == deleted {
                    false
                } else {
                    note.deleted = deleted;
                    true
                }
            } else {
                false
            }
        } else {
            false
        };
        self.persist_and_bump(changed);
        changed
    }

    pub fn clear_all(&self) -> bool {
        let changed = if let Ok(mut notes) = self.notes.lock() {
            let mut changed = false;
            for note in notes.iter_mut().filter(|note| !note.deleted) {
                note.deleted = true;
                changed = true;
            }
            changed
        } else {
            false
        };
        self.persist_and_bump(changed);
        changed
    }

    fn persist_and_bump(&self, changed: bool) {
        if !changed {
            return;
        }
        if let Some(dir) = Self::data_dir_from_store(&self.store) {
            if let Ok(notes) = self.notes.lock() {
                notes::save_notes(&dir, &notes);
            }
        }
        self.infra.bump();
    }

    fn data_dir_from_store(store: &Arc<Mutex<PodcastStore>>) -> Option<PathBuf> {
        store
            .lock()
            .ok()
            .and_then(|store| store.data_dir().map(PathBuf::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_note(id: &str) -> UserNote {
        UserNote {
            id: id.to_string(),
            text: "hello".to_string(),
            kind: "free".to_string(),
            target: None,
            created_at: 42,
            deleted: false,
            author: "user".to_string(),
        }
    }

    #[test]
    fn add_update_delete_and_project_note() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let state = NotesState::new(Infra::for_test(), store);

        assert!(state.add_note(sample_note("n1")));
        assert!(!state.add_note(sample_note("n1")));
        assert_eq!(state.notes_snapshot().len(), 1);

        assert!(state.update_note("n1", Some("edited".to_string()), None, None));
        assert_eq!(state.notes_snapshot()[0].text, "edited");

        assert!(state.set_deleted("n1", true));
        assert!(state.notes_snapshot()[0].deleted);
    }
}
