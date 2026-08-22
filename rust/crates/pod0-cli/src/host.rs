mod agent_http;
mod agent_payload;
pub(crate) mod capability;
mod config;
pub(crate) mod playback;
pub(crate) mod recall;
pub(crate) mod search;

pub use config::HostConfig;

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

impl HostExecutor {
    pub(crate) fn new(config: HostConfig) -> Result<Self, CliError> {
        let live = LiveHosts::new(ClientConfig::default())
            .map_err(|_| CliError::new("live_hosts", "live host client initialization failed", true))?;
        // Multi-thread (not current-thread): `HostExecutor` is constructed on
        // one thread but its `runtime.handle()` is driven via `block_on` from
        // the separate `pod0-host-pump` worker thread (see `app/host_loop.rs`).
        // A current-thread runtime's I/O/timer driver only makes progress when
        // driven from the thread that owns it, so `Handle::block_on` from any
        // other thread hangs forever; `self.runtime.block_on(...)` (called
        // directly on the owned `Runtime`, not a `Handle`) is unaffected and
        // keeps working either way.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|_| CliError::new("runtime", "async runtime initialization failed", true))?;
        Ok(Self { live, runtime, config })
    }

    pub(crate) fn config(&self) -> &HostConfig {
        &self.config
    }

    pub(crate) fn execute(&self, envelope: &HostRequestEnvelope) -> HostExecution {
        match &envelope.request {
            HostRequest::FetchFeed {
                feed_url,
                entity_tag,
                last_modified,
                maximum_response_bytes,
            } => HostExecution::Observed(Box::new(self.fetch_feed(
                feed_url,
                entity_tag.as_deref(),
                last_modified.as_deref(),
                *maximum_response_bytes,
            ))),
            HostRequest::FetchLibraryDocument {
                workflow_command_id,
                step,
                url,
                accept,
                maximum_response_bytes,
            } => HostExecution::Observed(Box::new(self.fetch_library(
                *workflow_command_id,
                step,
                url,
                accept,
                *maximum_response_bytes,
            ))),
            HostRequest::ExecuteAgentModelTurn { execution } => {
                HostExecution::Observed(Box::new(agent_http::execute(self, execution)))
            }
            HostRequest::LoadMedia { .. }
            | HostRequest::Play { .. }
            | HostRequest::Pause { .. }
            | HostRequest::Seek { .. }
            | HostRequest::SetRate { .. }
            | HostRequest::ObservePlayback { .. }
            | HostRequest::StopPlayback { .. }
            | HostRequest::ArmNativeTimer { .. }
            | HostRequest::CancelNativeTimer { .. } => HostExecution::Observed(Box::new(
                playback::execute(&envelope.request, self.runtime.handle()),
            )),
            HostRequest::PresentAgentApproval { approval } => {
                HostExecution::Observed(Box::new(HostObservation::AgentApprovalObserved {
                    turn_id: approval.turn_id,
                    proposal_id: approval.proposal.proposal_id,
                    proposal_digest: approval.proposal.proposal_digest,
                    decision: AgentApprovalDecision::Approve,
                }))
            }
            HostRequest::ExecuteAgentCapability { capability } => {
                HostExecution::Observed(Box::new(capability::execute(self, capability)))
            }
            HostRequest::ScheduleCoreWake { wake_at, reason } => {
                if now_milliseconds() >= wake_at.value {
                    HostExecution::Observed(Box::new(HostObservation::CoreWakeReached {
                        reason: *reason,
                    }))
                } else {
                    HostExecution::Pending
                }
            }
            HostRequest::EmbedRecallQuery {
                query_id,
                provider,
                model,
                text,
                maximum_dimensions,
            } => HostExecution::Observed(Box::new(recall::execute_query(
                self,
                *query_id,
                *provider,
                model.clone(),
                text.clone(),
                *maximum_dimensions,
            ))),
            HostRequest::EmbedRecallSpans {
                episode_id,
                generation_id,
                provider,
                model,
                spans,
                maximum_dimensions,
            } => HostExecution::Observed(Box::new(recall::execute_spans(
                self,
                *episode_id,
                *generation_id,
                *provider,
                model.clone(),
                spans.clone(),
                *maximum_dimensions,
            ))),
            HostRequest::RerankRecallCandidates {
                query_id,
                provider,
                model,
                query,
                candidates,
            } => HostExecution::Observed(Box::new(recall::execute_rerank(
                self,
                *query_id,
                *provider,
                model.clone(),
                query.clone(),
                candidates.clone(),
            ))),
            _ => HostExecution::Observed(Box::new(unsupported_observation(
                "this host request is unavailable in the headless host",
            ))),
        }
    }

    fn fetch_feed(
        &self,
        url: &str,
        entity_tag: Option<&str>,
        last_modified: Option<&str>,
        maximum_response_bytes: u64,
    ) -> HostObservation {
        let request = HttpGetRequest {
            url: url.to_owned(),
            accept: Some(
                "application/rss+xml, application/atom+xml, application/xml, text/xml;q=0.9, */*;q=0.1"
                    .to_owned(),
            ),
            entity_tag: entity_tag.map(str::to_owned),
            last_modified: last_modified.map(str::to_owned),
            options: RequestOptions {
                timeout: REQUEST_TIMEOUT,
                maximum_redirects: MAXIMUM_REDIRECTS,
                limits: HttpLimits {
                    maximum_body_bytes: maximum_response_bytes,
                    maximum_metadata_bytes: MAXIMUM_METADATA_BYTES,
                },
            },
        };
        match self
            .runtime
            .block_on(self.live.http_get(request, &CancellationToken::new()))
        {
            Ok(response) if response.evidence.status == 304 => HostObservation::FeedNotModified {
                entity_tag: response.evidence.entity_tag,
                last_modified: response.evidence.last_modified,
                response_url: response.evidence.final_url,
            },
            Ok(response) if !(200..300).contains(&response.evidence.status) => {
                status_failure(response.evidence.status, false)
            }
            Ok(response) => HostObservation::FeedBytesFetched {
                bytes: response.body,
                entity_tag: response.evidence.entity_tag,
                last_modified: response.evidence.last_modified,
                response_url: response.evidence.final_url,
                http_status: response.evidence.status,
            },
            Err(error) => map_adapter_error(&error),
        }
    }

    fn fetch_library(
        &self,
        workflow_command_id: pod0_facade::CommandId,
        step: &LibraryNetworkStep,
        url: &str,
        accept: &str,
        maximum_response_bytes: u64,
    ) -> HostObservation {
        let request = HttpGetRequest {
            url: url.to_owned(),
            accept: Some(accept.to_owned()),
            entity_tag: None,
            last_modified: None,
            options: RequestOptions {
                timeout: REQUEST_TIMEOUT,
                maximum_redirects: MAXIMUM_REDIRECTS,
                limits: HttpLimits {
                    maximum_body_bytes: maximum_response_bytes,
                    maximum_metadata_bytes: MAXIMUM_METADATA_BYTES,
                },
            },
        };
        match self
            .runtime
            .block_on(self.live.http_get(request, &CancellationToken::new()))
        {
            Ok(response) if !(200..300).contains(&response.evidence.status) => {
                status_failure(response.evidence.status, false)
            }
            Ok(response) => HostObservation::LibraryDocumentFetched {
                workflow_command_id,
                step: step.clone(),
                bytes: response.body,
                response_url: response.evidence.final_url,
                mime_type: response.evidence.content_type,
                http_status: response.evidence.status,
            },
            Err(error) => map_adapter_error(&error),
        }
    }
}

pub(super) fn failed(code: HostFailureCode, detail: &str) -> HostObservation {
    HostObservation::Failed {
        code,
        safe_detail: Some(detail.to_owned()),
    }
}

pub(super) fn status_failure(status: u16, provider: bool) -> HostObservation {
    let code = match status {
        401 | 403 => HostFailureCode::Unauthorized,
        408 | 504 => HostFailureCode::TimedOut,
        413 => HostFailureCode::ResponseTooLarge,
        429 | 500..=599 if provider => HostFailureCode::ProviderUnavailable,
        500..=599 => HostFailureCode::Offline,
        _ => HostFailureCode::InvalidResponse,
    };
    failed(code, "HTTP endpoint returned an unsuccessful status")
}

/// Maps a `pod0-live-hosts` adapter error to the durable `HostObservation`
/// failure shape, mirroring `status_failure`/`network_failure`'s existing
/// code choices for the blocking-client call sites this migration replaces.
pub(super) fn map_adapter_error(error: &AdapterError) -> HostObservation {
    match error {
        AdapterError::Network(network) => {
            let code = if matches!(network.kind, NetworkErrorKind::Timeout) {
                HostFailureCode::TimedOut
            } else {
                HostFailureCode::Offline
            };
            failed(code, "network request failed")
        }
        AdapterError::Provider(provider) => status_failure(provider.status, true),
        AdapterError::Credential(_) => failed(
            HostFailureCode::Unauthorized,
            "provider credential is unavailable",
        ),
        AdapterError::Protocol(_) | AdapterError::Size(_) => failed(
            HostFailureCode::InvalidResponse,
            "provider returned an invalid response",
        ),
        AdapterError::Unavailable(_) | AdapterError::File(_) | AdapterError::Cancelled => {
            failed(HostFailureCode::ProviderUnavailable, "provider is unavailable")
        }
        _ => failed(HostFailureCode::ProviderUnavailable, "provider request failed"),
    }
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
