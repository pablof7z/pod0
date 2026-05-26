//! Agent-memory (feature #33) accessors for [`super::PodcastStore`].
//!
//! Extracted so the memory-bag operations can grow independently as
//! the AI-memory feature adds query / filtering ops without bloating
//! `store/mod.rs`.
//!
//! Persistence is handled by the parent module's `persist()` helper —
//! every mutator here calls `self.persist()` so changes survive restart.

use crate::ffi::projections::MemoryFact;

use super::PodcastStore;

impl PodcastStore {
    /// Upsert a memory fact keyed on `key`. When a fact with the same key
    /// already exists, only the value and source change — the original
    /// `created_at` and `id` are preserved so the UI sees stable identity
    /// across edits.
    ///
    /// `source` is taken verbatim; the action handler is responsible for
    /// defaulting it (typically to `"user"`).
    pub fn set_memory_fact(&mut self, key: String, value: String, source: String, now_unix: i64) {
        let fact = match self.memory_facts.get(&key) {
            Some(existing) => MemoryFact {
                id: existing.id.clone(),
                key: existing.key.clone(),
                value,
                source,
                created_at: existing.created_at,
            },
            None => MemoryFact {
                id: key.clone(),
                key: key.clone(),
                value,
                source,
                created_at: now_unix,
            },
        };
        self.memory_facts.insert(key, fact);
        self.persist();
    }

    /// Delete a memory fact by key. Returns `true` when a row was removed
    /// so the caller can decide whether to bump `rev`.
    pub fn remove_memory_fact(&mut self, key: &str) -> bool {
        let removed = self.memory_facts.remove(key).is_some();
        if removed {
            self.persist();
        }
        removed
    }

    /// Wipe the entire memory bag. Returns the number of facts that were
    /// removed so the caller can decide whether to bump `rev`.
    pub fn clear_memory(&mut self) -> usize {
        let n = self.memory_facts.len();
        if n > 0 {
            self.memory_facts.clear();
            self.persist();
        }
        n
    }

    /// Snapshot of every memory fact, sorted by `key` so the iOS list is
    /// stable across re-renders without a client-side sort.
    pub fn all_memory_facts(&self) -> Vec<MemoryFact> {
        let mut facts: Vec<MemoryFact> = self.memory_facts.values().cloned().collect();
        facts.sort_by(|a, b| a.key.cmp(&b.key));
        facts
    }
}
