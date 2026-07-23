fn episode_json(episode: &pod0_domain::EpisodeRecord) -> serde_json::Value {
    json!({
        "episode_id": opaque_id_string(episode.episode_id.into_bytes()),
        "podcast_id": opaque_id_string(episode.podcast_id.into_bytes()),
        "title": episode.title,
        "position_milliseconds": episode.listening.resume_position_milliseconds,
        "completed": matches!(episode.listening.completion, CompletionStatus::Completed { .. })
    })
}

fn commit_fingerprint(domain: &str, proposal_id: pod0_domain::AgentProposalId) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-internal-commit:v1\0");
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(proposal_id.into_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn opaque_id_string(bytes: [u8; 16]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
