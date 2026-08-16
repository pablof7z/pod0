fn observation_fingerprint(input: &AgentCapabilityObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/capability-effect-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.lease.lease_id.into_bytes());
    hash.update(input.lease.fence.to_be_bytes());
    hash.update(serde_json::to_vec(&input.observation).expect("typed durable observation"));
    ContentDigest::from_bytes(hash.finalize().into())
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::AgentTurnConflict,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit agent capability effect observation",
        },
    }
}
