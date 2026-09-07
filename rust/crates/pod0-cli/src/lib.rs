#![forbid(unsafe_code)]

mod app;
mod host;
mod ids;
mod mapping;
mod protocol;
mod runner;
mod settings;

pub use app::Shell;
pub use host::HostConfig;
pub use pod0_facade::{HostObservation, HostRequestEnvelope};
pub use protocol::{CliRequest, CliResponse, PROTOCOL_VERSION};
pub use runner::{CliMode, run};

use host::{HostExecution, HostExecutor};

/// Execute a single host request against a one-shot headless host.
///
/// Returns `Ok(Some(observation))` when the host produced a real observation,
/// `Ok(None)` when the request is not yet due (for example a scheduled wake),
/// and `Err(message)` when the host executor could not be initialized.
///
/// This is the real-effect entry point used by agents and integration tests to
/// drive a single portable host capability without the full shell pump.
pub fn execute_host_request(
    config: HostConfig,
    envelope: HostRequestEnvelope,
) -> Result<Option<HostObservation>, String> {
    let host = HostExecutor::new(config).map_err(|error| error.message)?;
    match host.execute(&envelope) {
        HostExecution::Observed(observation) => Ok(Some(*observation)),
        HostExecution::Pending => Ok(None),
    }
}
