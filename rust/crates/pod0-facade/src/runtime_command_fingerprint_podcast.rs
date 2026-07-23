fn hash_podcast_upsert(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::UpsertSyntheticPodcast { podcast } => {
            hash.update(b"synthetic-podcast\0");
            match podcast.podcast_id {
                Some(id) => {
                    hash.update([1]);
                    hash.update(id.into_bytes());
                }
                None => hash.update([0]),
            }
            hash.update(podcast.title.as_bytes());
            hash.update([0]);
            hash.update(podcast.author.as_bytes());
            hash_optional(hash, podcast.image_url.as_deref());
            hash.update(podcast.description.as_bytes());
            hash_optional(hash, podcast.language.as_deref());
            hash.update((podcast.categories.len() as u64).to_be_bytes());
            for category in &podcast.categories {
                hash.update(category.as_bytes());
                hash.update([0]);
            }
        }
        ApplicationCommand::UpsertExternalEpisode { episode } => {
            hash.update(b"external-episode\0");
            hash.update(episode.podcast_id.into_bytes());
            hash_optional(hash, episode.feed_url.as_deref());
            hash.update(episode.podcast_title.as_bytes());
            hash.update([0]);
            hash.update(episode.audio_url.as_bytes());
            hash.update([0]);
            hash.update(episode.title.as_bytes());
            hash.update([0]);
            hash.update(episode.description.as_bytes());
            hash.update(episode.published_at.value.to_be_bytes());
            hash_optional(hash, episode.enclosure_mime_type.as_deref());
            hash_optional(hash, episode.image_url.as_deref());
            hash.update(
                episode
                    .duration_milliseconds
                    .unwrap_or(u64::MAX)
                    .to_be_bytes(),
            );
        }
        _ => unreachable!("podcast upsert fingerprint"),
    }
}
