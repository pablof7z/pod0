fn apply(
    transaction: &rusqlite::Transaction<'_>,
    internal_command_id: pod0_domain::InternalCommandId,
    fingerprint: &str,
    category_id: CategoryId,
    action: &AgentToolAction,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let command_id = CommandId::from_bytes(internal_command_id.into_bytes());
    match action {
        AgentToolAction::WriteCategory {
            category_id: None,
            name,
            description,
            color_hex,
            delete: false,
        } => crate::library_store_categories::create_category_in_transaction(
            transaction,
            command_id,
            fingerprint,
            category_id,
            name.as_deref().ok_or(StorageError::InvalidCategory)?,
            description.as_deref().ok_or(StorageError::InvalidCategory)?,
            color_hex.as_deref(),
            CategoryOrigin::Agent,
            observed_at_ms,
        ),
        AgentToolAction::WriteCategory {
            category_id: Some(_),
            delete: true,
            ..
        } => crate::library_store_categories::delete_category_in_transaction(
            transaction,
            command_id,
            fingerprint,
            category_id,
            observed_at_ms,
        ),
        AgentToolAction::WriteCategory {
            category_id: Some(_),
            name,
            description,
            color_hex,
            delete: false,
        } => crate::library_store_categories::update_category_in_transaction(
            transaction,
            command_id,
            fingerprint,
            category_id,
            &CategoryEdit {
                name: name.clone(),
                description: description.clone(),
                color_hex: color_hex.clone(),
            },
            observed_at_ms,
        ),
        AgentToolAction::TagItems {
            add_item_ids,
            remove_item_ids,
            ..
        } => {
            let add = add_item_ids
                .iter()
                .map(|item| {
                    resolve_item(transaction, *item)?
                        .map(|kind| (*item, kind))
                        .ok_or(StorageError::EntityNotFound)
                })
                .collect::<Result<Vec<_>, _>>()?;
            crate::library_store_category_members::tag_category_items_in_transaction(
                transaction,
                command_id,
                fingerprint,
                category_id,
                &add,
                remove_item_ids,
                observed_at_ms,
            )
            .map(|value| value.0)
        }
        _ => Err(StorageError::InvalidActivity),
    }
}

fn resolve_item(
    transaction: &rusqlite::Transaction<'_>,
    item_id: LibraryItemId,
) -> Result<Option<CategoryItemKind>, StorageError> {
    let (podcast, episode): (bool, bool) = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_podcasts WHERE podcast_id=?1),EXISTS(SELECT 1 FROM pod0_episodes WHERE episode_id=?1)",
            [item_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("resolve category item", error))?;
    match (podcast, episode) {
        (true, false) => Ok(Some(CategoryItemKind::Podcast)),
        (false, true) => Ok(Some(CategoryItemKind::Episode)),
        (false, false) => Ok(None),
        (true, true) => Err(StorageError::InvalidCategory),
    }
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (crate::category_store_read::collection_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn action_fingerprint(
    command_id: pod0_domain::InternalCommandId,
    action: &AgentToolAction,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/category/v1");
    hash.update(command_id.into_bytes());
    hash.update(serde_json::to_vec(action).expect("typed agent category action"));
    ContentDigest::from_bytes(hash.finalize().into())
}
