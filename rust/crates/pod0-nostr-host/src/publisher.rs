use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pod0_application::{LeasedNMPPublicationDraft, PersistedEffectLeaseIdentity};
use pod0_domain::PublicationId;

use crate::{
    AcknowledgedPublication, CancellationToken, ConfigurationError, PublicationFailure,
    PublishError, PublisherConfig, SigningSecret,
    config::{RelaySecurity, RelayTarget, validated_targets},
    event::sign_exact_draft,
    relay::{AttemptResult, Deadline, StopReason, publish_to_relay},
};

/// A configured blocking executor for exact leased Pod0 drafts.
pub struct NostrPublisher {
    targets: Vec<RelayTarget>,
    required_acknowledgements: usize,
    config: PublisherConfig,
    signing_secret: SigningSecret,
    // `Some` only when this publisher built its own `Runtime` (`NostrPublisher::new`);
    // `None` when driven by a caller-owned `Handle` (`NostrPublisher::new_with_handle`),
    // in which case the caller is responsible for that runtime's lifecycle. Mirrors
    // `pod0-portable-media`'s `HttpTransport` dual-field shape.
    owned_runtime: Option<tokio::runtime::Runtime>,
    handle: tokio::runtime::Handle,
}

impl NostrPublisher {
    pub fn new<I, S>(
        relay_urls: I,
        signing_secret: SigningSecret,
        config: PublisherConfig,
    ) -> Result<Self, ConfigurationError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let (targets, required_acknowledgements) = validated_targets(
            relay_urls,
            &config,
            RelaySecurity::AllowInsecureNumericLoopback,
        )?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| ConfigurationError::RuntimeInitializationFailed)?;
        let handle = runtime.handle().clone();
        Ok(Self {
            targets,
            required_acknowledgements,
            config,
            signing_secret,
            owned_runtime: Some(runtime),
            handle,
        })
    }

    /// Like [`NostrPublisher::new`], but drives its relay I/O on the caller's
    /// existing `Handle` instead of building a second `Runtime` — used when a
    /// process already owns the one `Runtime` for the whole process and must
    /// avoid a colliding second one. The caller's runtime must support being
    /// driven by `Handle::block_on` from other threads (i.e. a multi-thread
    /// runtime), since this publisher does not own the runtime's I/O driver.
    pub fn new_with_handle<I, S>(
        relay_urls: I,
        signing_secret: SigningSecret,
        config: PublisherConfig,
        handle: tokio::runtime::Handle,
    ) -> Result<Self, ConfigurationError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let (targets, required_acknowledgements) = validated_targets(
            relay_urls,
            &config,
            RelaySecurity::AllowInsecureNumericLoopback,
        )?;
        Ok(Self {
            targets,
            required_acknowledgements,
            config,
            signing_secret,
            owned_runtime: None,
            handle,
        })
    }

    /// Signs and publishes the exact leased draft.
    ///
    /// Relays are attempted in configured order. `AtLeast(n)` returns as soon
    /// as `n` distinct matching positive acknowledgements exist; `All`
    /// necessarily attempts every relay. No retry loop is owned here.
    pub fn publish(
        &self,
        leased: LeasedNMPPublicationDraft,
        cancellation: &CancellationToken,
    ) -> Result<AcknowledgedPublication, PublishError> {
        let lease = leased.lease;
        let publication_id = leased.draft.publication_id;
        if cancellation.is_cancelled() {
            return Err(PublishError::Cancelled(Box::new(self.failure(
                lease,
                publication_id,
                None,
                Vec::new(),
            ))));
        }
        let lease_remaining = remaining_lease(lease).ok_or_else(|| {
            PublishError::LeaseExpired(Box::new(self.failure(
                lease,
                publication_id,
                None,
                Vec::new(),
            )))
        })?;
        let (operation_duration, deadline_stop) =
            if lease_remaining <= self.config.operation_timeout {
                (lease_remaining, StopReason::LeaseExpired)
            } else {
                (self.config.operation_timeout, StopReason::OperationTimedOut)
            };
        let operation = Deadline {
            at: Instant::now()
                .checked_add(operation_duration)
                .unwrap_or_else(Instant::now),
            elapsed: deadline_stop,
        };
        let signed = sign_exact_draft(&leased.draft, &self.signing_secret)
            .map_err(PublishError::InvalidDraft)?;
        let event_id_hex = signed.event.id.to_hex();
        let mut outcomes = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            if let Some(stop) = current_stop(cancellation, lease, operation) {
                return Err(self.stopped(
                    stop,
                    lease,
                    publication_id,
                    Some(event_id_hex),
                    outcomes,
                ));
            }
            let attempt = publish_to_relay(
                target,
                &signed,
                &self.signing_secret,
                &self.config,
                cancellation,
                lease,
                operation,
            );
            // When this publisher owns its `Runtime` (the `NostrPublisher::new` path),
            // drive it via `Runtime::block_on` directly rather than through a cloned
            // `Handle` — a current-thread runtime's I/O/timer driver only makes
            // progress on the thread driving it, and `Runtime::block_on` (unlike
            // `Handle::block_on`) works correctly regardless of which thread calls
            // it. Only the caller-owned-runtime path (`new_with_handle`) uses the
            // `Handle`, and it is that caller's responsibility to ensure its runtime
            // supports being driven by `Handle::block_on` from other threads.
            let AttemptResult { outcome, stop } = match &self.owned_runtime {
                Some(runtime) => runtime.block_on(attempt),
                None => self.handle.block_on(attempt),
            };
            if let Some(outcome) = outcome {
                outcomes.push(outcome);
            }
            if let Some(stop) = stop {
                return Err(self.stopped(
                    stop,
                    lease,
                    publication_id,
                    Some(event_id_hex),
                    outcomes,
                ));
            }
            let acknowledgement_count = outcomes
                .iter()
                .filter(|outcome| outcome.is_acknowledged())
                .count();
            if acknowledgement_count >= self.required_acknowledgements {
                if remaining_lease(lease).is_none() {
                    return Err(PublishError::LeaseExpired(Box::new(self.failure(
                        lease,
                        publication_id,
                        Some(event_id_hex),
                        outcomes,
                    ))));
                }
                return Ok(AcknowledgedPublication {
                    lease,
                    publication_id,
                    event_id_hex,
                    required_acknowledgements: self.required_acknowledgements,
                    relay_outcomes: outcomes,
                });
            }
        }
        if let Some(stop) = current_stop(cancellation, lease, operation) {
            return Err(self.stopped(stop, lease, publication_id, Some(event_id_hex), outcomes));
        }
        Err(PublishError::AcknowledgementThresholdNotMet(Box::new(
            self.failure(lease, publication_id, Some(event_id_hex), outcomes),
        )))
    }

    fn stopped(
        &self,
        stop: StopReason,
        lease: PersistedEffectLeaseIdentity,
        publication_id: PublicationId,
        event_id_hex: Option<String>,
        outcomes: Vec<crate::RelayOutcome>,
    ) -> PublishError {
        let failure = self.failure(lease, publication_id, event_id_hex, outcomes);
        match stop {
            StopReason::Cancelled => PublishError::Cancelled(Box::new(failure)),
            StopReason::LeaseExpired => PublishError::LeaseExpired(Box::new(failure)),
            StopReason::OperationTimedOut => PublishError::OperationTimedOut(Box::new(failure)),
        }
    }

    fn failure(
        &self,
        lease: PersistedEffectLeaseIdentity,
        publication_id: PublicationId,
        event_id_hex: Option<String>,
        relay_outcomes: Vec<crate::RelayOutcome>,
    ) -> PublicationFailure {
        PublicationFailure {
            lease,
            publication_id,
            event_id_hex,
            required_acknowledgements: self.required_acknowledgements,
            relay_outcomes,
        }
    }
}

fn current_stop(
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> Option<StopReason> {
    if cancellation.is_cancelled() {
        Some(StopReason::Cancelled)
    } else if remaining_lease(lease).is_none() {
        Some(StopReason::LeaseExpired)
    } else if Instant::now() >= operation.at {
        Some(operation.elapsed)
    } else {
        None
    }
}

fn remaining_lease(lease: PersistedEffectLeaseIdentity) -> Option<Duration> {
    let remaining = lease.expires_at.value.checked_sub(unix_milliseconds())?;
    (remaining > 0).then(|| Duration::from_millis(remaining as u64))
}

pub(crate) fn unix_milliseconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "publisher_tests.rs"]
mod tests;
