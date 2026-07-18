use pod0_application::ApplicationCommand;
use pod0_domain::{AutoDownloadMode, AutoDownloadPolicy};
use sha2::{Digest, Sha256};

pub(super) fn command_fingerprint(command: &ApplicationCommand) -> String {
    let mut hash = Sha256::new();
    match command {
        ApplicationCommand::SubscribeToFeed { feed_url } => {
            hash.update(b"subscribe\0");
            hash.update(feed_url.as_bytes());
        }
        ApplicationCommand::EnsurePodcast { feed_url } => {
            hash.update(b"ensure\0");
            hash.update(feed_url.as_bytes());
        }
        ApplicationCommand::RefreshPodcast { podcast_id } => {
            hash.update(b"refresh\0");
            hash.update(podcast_id.into_bytes());
        }
        ApplicationCommand::HydratePodcastMetadata { podcast_id } => {
            hash.update(b"hydrate-metadata\0");
            hash.update(podcast_id.into_bytes());
        }
        ApplicationCommand::UpsertExternalEpisode {
            podcast_id,
            feed_url,
            podcast_title,
            audio_url,
            title,
            image_url,
            duration_milliseconds,
        } => {
            hash.update(b"external-episode\0");
            hash.update(podcast_id.into_bytes());
            hash_optional(&mut hash, feed_url.as_deref());
            hash.update(podcast_title.as_bytes());
            hash.update([0]);
            hash.update(audio_url.as_bytes());
            hash.update([0]);
            hash.update(title.as_bytes());
            hash_optional(&mut hash, image_url.as_deref());
            hash.update(duration_milliseconds.unwrap_or(u64::MAX).to_be_bytes());
        }
        ApplicationCommand::Unsubscribe { podcast_id } => {
            hash.update(b"unsubscribe\0");
            hash.update(podcast_id.into_bytes());
        }
        ApplicationCommand::SetSubscriptionNotifications {
            podcast_id,
            enabled,
        } => {
            hash.update(b"notifications\0");
            hash.update(podcast_id.into_bytes());
            hash.update([u8::from(*enabled)]);
        }
        ApplicationCommand::SetSubscriptionAutoDownload { podcast_id, policy } => {
            hash.update(b"auto-download\0");
            hash.update(podcast_id.into_bytes());
            hash_policy(&mut hash, policy);
        }
        ApplicationCommand::RequestPlayback { episode_id } => {
            hash.update(b"play\0");
            hash.update(episode_id.into_bytes());
        }
        ApplicationCommand::CancelOperation { cancellation_id } => {
            hash.update(b"cancel\0");
            hash.update(cancellation_id.into_bytes());
        }
        ApplicationCommand::Unsupported { wire_code } => {
            hash.update(b"unsupported\0");
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_optional(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash.update(value.as_bytes());
        }
        None => hash.update([0]),
    }
    hash.update([0]);
}

fn hash_policy(hash: &mut Sha256, policy: &AutoDownloadPolicy) {
    match policy.mode {
        AutoDownloadMode::Off => hash.update([1]),
        AutoDownloadMode::Latest { count } => {
            hash.update([2]);
            hash.update(count.to_be_bytes());
        }
        AutoDownloadMode::AllNew => hash.update([3]),
        AutoDownloadMode::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.update([u8::from(policy.wifi_only)]);
}
