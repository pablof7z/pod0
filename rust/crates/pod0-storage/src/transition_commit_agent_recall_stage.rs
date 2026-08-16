fn stage(
    transaction: &rusqlite::Transaction<'_>,
    input: &AgentRecallObservationCommitInput,
    fingerprint: ContentDigest,
) -> Result<(), StorageError> {
    let changed = transaction.execute(
        "UPDATE pod0_effect_attempts SET state_code=2,observation_schema_version=1,\
         observation_json=?1,outcome_schema_version=1,outcome_json=?2,observed_at_ms=?3 \
         WHERE lease_id=?4 AND fence=?5 AND state_code=1",
        params![
            fingerprint.into_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            serde_json::to_string(&observation_outcome(&input.observation.outcome))
                .map_err(|_| StorageError::InvalidActivity)?,
            input.observation.observed_at.value,
            input.lease.lease_id.into_bytes().as_slice(),
            i64::try_from(input.lease.fence).map_err(|_| StorageError::InvalidActivity)?,
        ],
    ).map_err(|error| StorageError::sqlite("stage agent recall observation", error))?;
    (changed == 1).then_some(()).ok_or(StorageError::AgentTurnConflict)
}

fn fingerprint(input: &AgentRecallObservationCommitInput) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent-recall/observation/v1");
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(serde_json::to_vec(&input.observation).map_err(|_| StorageError::InvalidActivity)?);
    hash.update(serde_json::to_vec(&input.resolution).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}
