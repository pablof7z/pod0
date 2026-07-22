use pod0_application::{ScheduledAgentFailure, ScheduledAgentFailureCode, ScheduledAgentStage};
use pod0_domain::{
    CancellationId, CommandId, ContentDigest, GeneratedArtifactId, HostRequestId,
    ScheduledAttemptId, ScheduledOccurrenceId, ScheduledTaskId, StateRevision,
};

use crate::StorageError;

macro_rules! id_decoder {
    ($name:ident, $type:ty, $detail:literal) => {
        pub(crate) fn $name(bytes: &[u8]) -> Result<$type, StorageError> {
            let value: [u8; 16] = bytes
                .try_into()
                .map_err(|_| StorageError::CorruptSchema { detail: $detail })?;
            Ok(<$type>::from_bytes(value))
        }
    };
}

id_decoder!(
    task_id,
    ScheduledTaskId,
    "scheduled task identity is malformed"
);
id_decoder!(
    occurrence_id,
    ScheduledOccurrenceId,
    "scheduled occurrence identity is malformed"
);
id_decoder!(
    attempt_id,
    ScheduledAttemptId,
    "scheduled attempt identity is malformed"
);
id_decoder!(
    request_id,
    HostRequestId,
    "scheduled request identity is malformed"
);
id_decoder!(
    command_id,
    CommandId,
    "scheduled command identity is malformed"
);
id_decoder!(
    cancellation_id,
    CancellationId,
    "scheduled cancellation identity is malformed"
);
id_decoder!(
    artifact_id,
    GeneratedArtifactId,
    "generated artifact identity is malformed"
);

pub(crate) fn digest(bytes: &[u8]) -> Result<ContentDigest, StorageError> {
    let value: [u8; 32] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "scheduled content digest is malformed",
    })?;
    Ok(ContentDigest::from_bytes(value))
}

pub(crate) fn revision(value: i64) -> Result<StateRevision, StorageError> {
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "scheduled revision is malformed",
        }
    })?))
}

pub(crate) const fn stage_wire(stage: ScheduledAgentStage) -> Option<&'static str> {
    match stage {
        ScheduledAgentStage::Pending => Some("pending"),
        ScheduledAgentStage::Requested => Some("requested"),
        ScheduledAgentStage::HostAccepted => Some("host_accepted"),
        ScheduledAgentStage::RetryScheduled => Some("retry_scheduled"),
        ScheduledAgentStage::Blocked => Some("blocked"),
        ScheduledAgentStage::Cancelled => Some("cancelled"),
        ScheduledAgentStage::Obsolete => Some("obsolete"),
        ScheduledAgentStage::FailedPermanent => Some("failed_permanent"),
        ScheduledAgentStage::Succeeded => Some("succeeded"),
        ScheduledAgentStage::Ambiguous => Some("ambiguous"),
        ScheduledAgentStage::Unsupported { .. } => None,
    }
}

pub(crate) fn parse_stage(value: &str) -> Result<ScheduledAgentStage, StorageError> {
    match value {
        "pending" => Ok(ScheduledAgentStage::Pending),
        "requested" => Ok(ScheduledAgentStage::Requested),
        "host_accepted" => Ok(ScheduledAgentStage::HostAccepted),
        "retry_scheduled" => Ok(ScheduledAgentStage::RetryScheduled),
        "blocked" => Ok(ScheduledAgentStage::Blocked),
        "cancelled" => Ok(ScheduledAgentStage::Cancelled),
        "obsolete" => Ok(ScheduledAgentStage::Obsolete),
        "failed_permanent" => Ok(ScheduledAgentStage::FailedPermanent),
        "succeeded" => Ok(ScheduledAgentStage::Succeeded),
        "ambiguous" => Ok(ScheduledAgentStage::Ambiguous),
        _ => Err(StorageError::CorruptSchema {
            detail: "scheduled stage is malformed",
        }),
    }
}

pub(crate) const fn failure_wire(code: ScheduledAgentFailureCode) -> (&'static str, Option<i64>) {
    match code {
        ScheduledAgentFailureCode::MissingCredential => ("missing_credential", None),
        ScheduledAgentFailureCode::Offline => ("offline", None),
        ScheduledAgentFailureCode::Network => ("network", None),
        ScheduledAgentFailureCode::RateLimited => ("rate_limited", None),
        ScheduledAgentFailureCode::ProviderUnavailable => ("provider_unavailable", None),
        ScheduledAgentFailureCode::PermissionDenied => ("permission_denied", None),
        ScheduledAgentFailureCode::InvalidOutput => ("invalid_output", None),
        ScheduledAgentFailureCode::UnsafeToRetry => ("unsafe_to_retry", None),
        ScheduledAgentFailureCode::Cancelled => ("cancelled", None),
        ScheduledAgentFailureCode::Unexpected => ("unexpected", None),
        ScheduledAgentFailureCode::RetryExhausted => ("retry_exhausted", None),
        ScheduledAgentFailureCode::StorageUnavailable => ("storage_unavailable", None),
        ScheduledAgentFailureCode::Unsupported { wire_code } => {
            ("unsupported", Some(wire_code as i64))
        }
    }
}

pub(crate) fn parse_failure(
    code: Option<String>,
    wire: Option<i64>,
    detail: Option<String>,
    retryable: bool,
) -> Result<Option<ScheduledAgentFailure>, StorageError> {
    let Some(code) = code else {
        if wire.is_none() && detail.is_none() && !retryable {
            return Ok(None);
        }
        return Err(StorageError::CorruptSchema {
            detail: "scheduled failure is malformed",
        });
    };
    let value = match code.as_str() {
        "missing_credential" => ScheduledAgentFailureCode::MissingCredential,
        "offline" => ScheduledAgentFailureCode::Offline,
        "network" => ScheduledAgentFailureCode::Network,
        "rate_limited" => ScheduledAgentFailureCode::RateLimited,
        "provider_unavailable" => ScheduledAgentFailureCode::ProviderUnavailable,
        "permission_denied" => ScheduledAgentFailureCode::PermissionDenied,
        "invalid_output" => ScheduledAgentFailureCode::InvalidOutput,
        "unsafe_to_retry" => ScheduledAgentFailureCode::UnsafeToRetry,
        "cancelled" => ScheduledAgentFailureCode::Cancelled,
        "unexpected" => ScheduledAgentFailureCode::Unexpected,
        "retry_exhausted" => ScheduledAgentFailureCode::RetryExhausted,
        "storage_unavailable" => ScheduledAgentFailureCode::StorageUnavailable,
        "unsupported" => ScheduledAgentFailureCode::Unsupported {
            wire_code: u32::try_from(wire.ok_or(StorageError::CorruptSchema {
                detail: "scheduled failure wire code is missing",
            })?)
            .map_err(|_| StorageError::CorruptSchema {
                detail: "scheduled failure wire code is malformed",
            })?,
        },
        _ => {
            return Err(StorageError::CorruptSchema {
                detail: "scheduled failure code is malformed",
            });
        }
    };
    Ok(Some(ScheduledAgentFailure {
        code: value,
        safe_detail: detail,
        retryable,
    }))
}
