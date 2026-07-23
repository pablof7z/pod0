impl FacadeState {
    fn notes_projection(
        &self,
        scope: NoteProjectionScope,
        offset: usize,
        item_limit: usize,
    ) -> Projection {
        let mut notes = self.notes.notes.clone();
        match scope {
            NoteProjectionScope::All => {}
            NoteProjectionScope::Active => notes.retain(|note| !note.deleted),
            NoteProjectionScope::Episode { episode_id } => {
                notes.retain(|note| {
                    !note.deleted
                        && matches!(
                            note.target,
                            Some(pod0_domain::NoteTarget::Episode {
                                episode_id: id,
                                ..
                            }) if id == episode_id
                        )
                });
                notes.sort_by_key(|note| {
                    let position = match note.target {
                        Some(pod0_domain::NoteTarget::Episode {
                            position_milliseconds,
                            ..
                        }) => position_milliseconds,
                        _ => u64::MAX,
                    };
                    (position, note.created_at.value, note.note_id)
                });
            }
            NoteProjectionScope::Unsupported { .. } => notes.clear(),
        }
        let mut value = NotesProjection {
            scope,
            collection_revision: self.notes.revision,
            notes,
            operations: self.operations.clone(),
            has_more: false,
        };
        value.enforce_bounds(offset, item_limit);
        Projection::Notes { value }
    }
}
