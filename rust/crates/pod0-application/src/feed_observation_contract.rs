use pod0_domain::{
    CancellationId, EpisodeId, FeedDiscoveryOccurrenceId, HostRequestId, StateRevision,
    UnixTimestampMilliseconds,
};

use crate::{HostFailureCode, HostObservation, HostObservationEnvelope};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableFeedHostObservation {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub outcome: DurableFeedObservationOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableFeedObservationOutcome {
    Fetched {
        bytes: Vec<u8>,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        response_url: String,
        http_status: u16,
    },
    NotModified {
        entity_tag: Option<String>,
        last_modified: Option<String>,
        response_url: String,
    },
    Failed {
        code: HostFailureCode,
    },
    NotificationDelivered {
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
    },
    Cancelled,
}

impl DurableFeedHostObservation {
    #[must_use]
    pub fn from_host(value: &HostObservationEnvelope) -> Option<Self> {
        let outcome = match &value.observation {
            HostObservation::FeedBytesFetched {
                bytes,
                entity_tag,
                last_modified,
                response_url,
                http_status,
            } => DurableFeedObservationOutcome::Fetched {
                bytes: bytes.clone(),
                entity_tag: entity_tag.clone(),
                last_modified: last_modified.clone(),
                response_url: response_url.clone(),
                http_status: *http_status,
            },
            HostObservation::FeedNotModified {
                entity_tag,
                last_modified,
                response_url,
            } => DurableFeedObservationOutcome::NotModified {
                entity_tag: entity_tag.clone(),
                last_modified: last_modified.clone(),
                response_url: response_url.clone(),
            },
            HostObservation::Failed { code, .. } => {
                DurableFeedObservationOutcome::Failed { code: *code }
            }
            HostObservation::Cancelled => DurableFeedObservationOutcome::Cancelled,
            HostObservation::NewEpisodeNotificationDelivered {
                occurrence_id,
                episode_id,
            } => DurableFeedObservationOutcome::NotificationDelivered {
                occurrence_id: *occurrence_id,
                episode_id: *episode_id,
            },
            _ => return None,
        };
        Some(Self {
            request_id: value.request_id,
            cancellation_id: value.cancellation_id,
            observed_request_revision: value.observed_request_revision,
            sequence_number: value.sequence_number,
            observed_at: value.observed_at,
            outcome,
        })
    }
}
