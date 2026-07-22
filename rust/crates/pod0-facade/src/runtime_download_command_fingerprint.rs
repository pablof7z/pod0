use pod0_application::{ApplicationCommand, DownloadIntentOrigin, DownloadNetworkState};
use sha2::{Digest as _, Sha256};

pub(super) fn hash_download_command(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::RequestEpisodeDownload { episode_id, origin } => {
            hash.update(b"request-download\0");
            hash.update(episode_id.into_bytes());
            hash_origin(hash, *origin);
        }
        ApplicationCommand::CancelEpisodeDownload {
            episode_id,
            expected_workflow_revision,
        } => {
            hash.update(b"cancel-download\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        ApplicationCommand::RemoveEpisodeDownload {
            episode_id,
            expected_workflow_revision,
        } => {
            hash.update(b"remove-download\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        ApplicationCommand::ObserveDownloadEnvironment { observation } => {
            hash.update(b"observe-download-environment\0");
            hash_network(hash, observation.network);
            hash.update(
                observation
                    .available_capacity_bytes
                    .unwrap_or(u64::MAX)
                    .to_be_bytes(),
            );
        }
        _ => unreachable!("download fingerprint called with another command"),
    }
}

fn hash_origin(hash: &mut Sha256, value: DownloadIntentOrigin) {
    match value {
        DownloadIntentOrigin::User => hash.update([1]),
        DownloadIntentOrigin::Playback => hash.update([2]),
        DownloadIntentOrigin::Automatic => hash.update([3]),
        DownloadIntentOrigin::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

fn hash_network(hash: &mut Sha256, value: DownloadNetworkState) {
    match value {
        DownloadNetworkState::Unknown => hash.update([1]),
        DownloadNetworkState::Unavailable => hash.update([2]),
        DownloadNetworkState::Wifi => hash.update([3]),
        DownloadNetworkState::Other => hash.update([4]),
        DownloadNetworkState::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}
