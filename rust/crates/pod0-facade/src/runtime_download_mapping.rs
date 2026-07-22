use pod0_application::{
    CoreFailureCode, DownloadAdmissionDecision, DownloadEnvironmentObservation,
    DownloadIntentOrigin, DownloadNetworkState, DownloadWaitReason,
};
use pod0_storage::{StoredDownloadNetwork, StoredDownloadOrigin};

pub(super) fn stored_origin(
    value: DownloadIntentOrigin,
) -> Result<StoredDownloadOrigin, CoreFailureCode> {
    match value {
        DownloadIntentOrigin::User => Ok(StoredDownloadOrigin::User),
        DownloadIntentOrigin::Playback => Ok(StoredDownloadOrigin::Playback),
        DownloadIntentOrigin::Automatic => Ok(StoredDownloadOrigin::Automatic),
        DownloadIntentOrigin::Unsupported { wire_code } => {
            Err(CoreFailureCode::Unsupported { wire_code })
        }
    }
}

pub(super) fn projected_origin(value: StoredDownloadOrigin) -> DownloadIntentOrigin {
    match value {
        StoredDownloadOrigin::User => DownloadIntentOrigin::User,
        StoredDownloadOrigin::Playback => DownloadIntentOrigin::Playback,
        StoredDownloadOrigin::Automatic => DownloadIntentOrigin::Automatic,
        StoredDownloadOrigin::Unsupported(wire_code) => {
            DownloadIntentOrigin::Unsupported { wire_code }
        }
    }
}

pub(super) fn stored_network(value: DownloadNetworkState) -> StoredDownloadNetwork {
    match value {
        DownloadNetworkState::Unknown => StoredDownloadNetwork::Unknown,
        DownloadNetworkState::Unavailable => StoredDownloadNetwork::Unavailable,
        DownloadNetworkState::Wifi => StoredDownloadNetwork::Wifi,
        DownloadNetworkState::Other => StoredDownloadNetwork::Other,
        DownloadNetworkState::Unsupported { wire_code } => {
            StoredDownloadNetwork::Unsupported(wire_code)
        }
    }
}

pub(super) fn environment_projection(
    value: pod0_storage::DownloadEnvironmentRecord,
) -> DownloadEnvironmentObservation {
    let network = match value.network {
        StoredDownloadNetwork::Unknown => DownloadNetworkState::Unknown,
        StoredDownloadNetwork::Unavailable => DownloadNetworkState::Unavailable,
        StoredDownloadNetwork::Wifi => DownloadNetworkState::Wifi,
        StoredDownloadNetwork::Other => DownloadNetworkState::Other,
        StoredDownloadNetwork::Unsupported(wire_code) => {
            DownloadNetworkState::Unsupported { wire_code }
        }
    };
    DownloadEnvironmentObservation {
        network,
        available_capacity_bytes: value.available_capacity_bytes,
    }
}

pub(super) fn wait_failure(value: DownloadAdmissionDecision) -> Option<&'static str> {
    match value {
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::NetworkUnknown,
        } => Some("network_unknown"),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::NetworkUnavailable,
        } => Some("offline"),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::WifiRequired,
        } => Some("wifi_required"),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::InsufficientStorage,
        } => Some("insufficient_storage"),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::UnsupportedEnvironment { .. },
        } => Some("unsupported_environment"),
        DownloadAdmissionDecision::Admit | DownloadAdmissionDecision::Obsolete => None,
    }
}
