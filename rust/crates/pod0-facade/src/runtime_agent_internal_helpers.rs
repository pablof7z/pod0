fn episode_json(episode: &pod0_domain::EpisodeRecord) -> serde_json::Value {
    json!({
        "episode_id": opaque_id_string(episode.episode_id.into_bytes()),
        "podcast_id": opaque_id_string(episode.podcast_id.into_bytes()),
        "title": episode.title,
        "position_milliseconds": episode.listening.resume_position_milliseconds,
        "completed": matches!(episode.listening.completion, CompletionStatus::Completed { .. })
    })
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
