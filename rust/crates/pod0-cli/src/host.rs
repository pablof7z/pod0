mod agent_http;
mod agent_payload;
pub(crate) mod capability;
mod config;
mod executor;
pub(crate) mod playback;
pub(crate) mod recall;
pub(crate) mod search;

pub use config::HostConfig;
use executor::{failed, map_adapter_error};

use std::time::Duration;

use pod0_application::LibraryNetworkStep;
use pod0_facade::{
    AgentApprovalDecision, HostFailureCode, HostObservation, HostRequest, HostRequestEnvelope,
};
use pod0_live_hosts::{
    AdapterError, CancellationToken, ClientConfig, HttpGetRequest, HttpLimits, LiveHosts,
    NetworkErrorKind, RequestOptions,
};

use crate::protocol::CliError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const MAXIMUM_REDIRECTS: u8 = 10;
const MAXIMUM_METADATA_BYTES: u64 = 64 * 1024;

pub(crate) enum HostExecution {
    Observed(Box<HostObservation>),
    Pending,
}

pub(crate) struct HostExecutor {
    live: LiveHosts,
    runtime: tokio::runtime::Runtime,
    config: HostConfig,
}

fn unsupported_observation(detail: &str) -> HostObservation {
    failed(HostFailureCode::Unsupported { wire_code: 1 }, detail)
}

fn now_milliseconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_capabilities_are_never_reported_as_success() {
        assert!(matches!(
            unsupported_observation("not implemented"),
            HostObservation::Failed {
                code: HostFailureCode::Unsupported { .. },
                ..
            }
        ));
    }
}
