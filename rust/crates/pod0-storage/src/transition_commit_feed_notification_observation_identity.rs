fn notification_observation_identity(
    attempt: pod0_domain::EffectAttemptId,
    sequence: u64,
) -> pod0_domain::EffectAttemptId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-feed-notification-observation-v1\0");
    hash.update(attempt.into_bytes());
    hash.update(sequence.to_be_bytes());
    pod0_domain::EffectAttemptId::from_bytes(hash.finalize()[..16].try_into().expect("digest"))
}

fn notification_observation_fingerprint(
    observation: &pod0_application::DurableFeedHostObservation,
) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(observation).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}
