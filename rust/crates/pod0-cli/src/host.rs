mod agent_http;
mod agent_payload;
mod config;
pub(crate) mod playback;
pub(crate) mod recall;
pub(crate) mod search;

pub use config::HostConfig;

use std::io::Read as _;
use std::time::Duration;

use pod0_facade::{
    AgentApprovalDecision, HostFailureCode, HostObservation, HostRequest, HostRequestEnvelope,
    LibraryNetworkStep,
};
use pod0_live_hosts::{ClientConfig, LiveHosts};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};

use crate::protocol::CliError;

pub(crate) enum HostExecution {
    Observed(Box<HostObservation>),
    Pending,
}

pub(crate) struct HostExecutor {
    client: Client,
    live: LiveHosts,
    runtime: tokio::runtime::Runtime,
    config: HostConfig,
}

impl HostExecutor {
    pub(crate) fn new(config: HostConfig) -> Result<Self, CliError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|_| CliError::new("http_client", "HTTP client initialization failed", true))?;
        let live = LiveHosts::new(ClientConfig::default())
            .map_err(|_| CliError::new("live_hosts", "live host client initialization failed", true))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| CliError::new("runtime", "async runtime initialization failed", true))?;
        Ok(Self { client, live, runtime, config })
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
            | HostRequest::CancelNativeTimer { .. } => {
                HostExecution::Observed(Box::new(playback::execute(&envelope.request)))
            }
            HostRequest::PresentAgentApproval { approval } => {
                HostExecution::Observed(Box::new(HostObservation::AgentApprovalObserved {
                    turn_id: approval.turn_id,
                    proposal_id: approval.proposal.proposal_id,
                    proposal_digest: approval.proposal.proposal_digest,
                    decision: AgentApprovalDecision::Deny,
                }))
            }
            HostRequest::ExecuteAgentCapability { .. } => {
                HostExecution::Observed(Box::new(unsupported_observation(
                    "agent capability execution is unavailable in the headless host",
                )))
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
        let mut request = self.client.get(url).header(
            ACCEPT,
            "application/rss+xml, application/atom+xml, application/xml, text/xml;q=0.9, */*;q=0.1",
        );
        if let Some(value) = entity_tag {
            request = request.header(IF_NONE_MATCH, value);
        }
        if let Some(value) = last_modified {
            request = request.header(IF_MODIFIED_SINCE, value);
        }
        let response = match request.send() {
            Ok(response) => response,
            Err(error) => return network_failure(&error, false),
        };
        let status = response.status();
        let response_url = response.url().to_string();
        let entity_tag = header(&response, ETAG);
        let last_modified = header(&response, LAST_MODIFIED);
        if status.as_u16() == 304 {
            return HostObservation::FeedNotModified {
                entity_tag,
                last_modified,
                response_url,
            };
        }
        if !status.is_success() {
            return status_failure(status.as_u16(), false);
        }
        match read_bounded(response, maximum_response_bytes) {
            Ok(bytes) => HostObservation::FeedBytesFetched {
                bytes,
                entity_tag,
                last_modified,
                response_url,
                http_status: status.as_u16(),
            },
            Err(observation) => *observation,
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
        let response = match self.client.get(url).header(ACCEPT, accept).send() {
            Ok(response) => response,
            Err(error) => return network_failure(&error, false),
        };
        let status = response.status();
        if !status.is_success() {
            return status_failure(status.as_u16(), false);
        }
        let response_url = response.url().to_string();
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        match read_bounded(response, maximum_response_bytes) {
            Ok(bytes) => HostObservation::LibraryDocumentFetched {
                workflow_command_id,
                step: step.clone(),
                bytes,
                response_url,
                mime_type,
                http_status: status.as_u16(),
            },
            Err(observation) => *observation,
        }
    }
}

pub(super) fn read_bounded(
    mut response: Response,
    maximum_response_bytes: u64,
) -> Result<Vec<u8>, Box<HostObservation>> {
    let mut bytes = Vec::new();
    if response
        .by_ref()
        .take(maximum_response_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(Box::new(failed(
            HostFailureCode::InvalidResponse,
            "response body could not be read",
        )));
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum_response_bytes {
        return Err(Box::new(failed(
            HostFailureCode::ResponseTooLarge,
            "response exceeded the core limit",
        )));
    }
    Ok(bytes)
}

pub(super) fn failed(code: HostFailureCode, detail: &str) -> HostObservation {
    HostObservation::Failed {
        code,
        safe_detail: Some(detail.to_owned()),
    }
}

pub(super) fn network_failure(error: &reqwest::Error, provider: bool) -> HostObservation {
    let code = if error.is_timeout() {
        HostFailureCode::TimedOut
    } else if provider {
        HostFailureCode::ProviderUnavailable
    } else {
        HostFailureCode::Offline
    };
    failed(code, "network request failed")
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

fn header(response: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
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
