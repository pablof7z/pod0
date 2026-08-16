fn apply_reset(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    for (operation, sql) in [
        ("reset listening queue", "DELETE FROM pod0_queue_entries"),
        ("reset clips", "DELETE FROM pod0_clips"),
        ("reset notes", "DELETE FROM pod0_notes"),
    ] {
        transaction
            .execute(sql, [])
            .map_err(|error| StorageError::sqlite(operation, error))?;
    }
    transaction
        .execute(
            "UPDATE pod0_playback_state SET active_episode_id=NULL,playback_rate_permille=1000,\
         sleep_mode_code=1,sleep_duration_ms=NULL,sleep_wire_code=NULL,\
         auto_mark_played_at_natural_end=1,auto_play_next=1,active_segment_start_ms=NULL,\
         active_segment_end_ms=NULL,active_segment_label=NULL,last_position_committed_at_ms=NULL \
         WHERE singleton=1",
            [],
        )
        .map_err(|error| StorageError::sqlite("reset listening playback", error))?;
    for (operation, sql) in [
        (
            "reset episode metadata",
            "DELETE FROM pod0_episode_feed_metadata",
        ),
        ("reset listening episodes", "DELETE FROM pod0_episodes"),
        ("reset subscriptions", "DELETE FROM pod0_subscriptions"),
        ("reset podcasts", "DELETE FROM pod0_podcasts"),
        (
            "reset command receipts",
            "DELETE FROM pod0_library_commands",
        ),
    ] {
        transaction
            .execute(sql, [])
            .map_err(|error| StorageError::sqlite(operation, error))?;
    }
    Ok(())
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (playback_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn playback_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read reset playback revision", error))?;
    u64::try_from(value)
        .map(StateRevision::new)
        .map_err(|_| StorageError::InvalidActivity)
}
