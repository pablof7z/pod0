use pod0_application::PersistedEffectLeaseIdentity;
use pod0_domain::{PublicationId, PublicationRouteId, UnixTimestampMilliseconds};

/// Transport phase in which a relay failed before a proven acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayFailureStage {
    Resolve,
    Connect,
    Handshake,
}

/// Stable, payload-free classification of a relay failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayFailureCode {
    Dns,
    ConnectionRefused,
    Timeout,
    Tls,
    Http,
    WebSocket,
    AuthenticationRejected,
    AuthenticationRequired,
    RelayRejected,
}

/// Why a sent event has no definitive relay result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmbiguityCause {
    Cancelled,
    LeaseExpired,
    OperationTimedOut,
    AcknowledgementTimedOut,
    SendFailed,
    AuthenticationFailed,
    Disconnected,
    ProtocolFailure,
}

/// Exact evidence observed for one configured relay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayOutcome {
    /// The relay returned `OK true` for the exact signed event id.
    Acknowledged {
        relay_url: String,
        route_id: PublicationRouteId,
        observed_at: UnixTimestampMilliseconds,
        message: String,
    },
    /// The relay returned a definitive `OK false`.
    Rejected {
        relay_url: String,
        route_id: PublicationRouteId,
        observed_at: UnixTimestampMilliseconds,
        code: RelayFailureCode,
        message: String,
    },
    /// No event handoff was completed at this relay.
    Failed {
        relay_url: String,
        route_id: PublicationRouteId,
        stage: RelayFailureStage,
        code: RelayFailureCode,
        detail: String,
    },
    /// Bytes may have reached the relay, but no matching definitive `OK` did.
    HandoffAmbiguous {
        relay_url: String,
        route_id: PublicationRouteId,
        cause: AmbiguityCause,
        detail: String,
    },
}

impl RelayOutcome {
    #[must_use]
    pub fn is_acknowledged(&self) -> bool {
        matches!(self, Self::Acknowledged { .. })
    }

    #[must_use]
    pub fn handoff_possible(&self) -> bool {
        matches!(
            self,
            Self::Acknowledged { .. } | Self::Rejected { .. } | Self::HandoffAmbiguous { .. }
        )
    }
}

/// Successful publication evidence.
///
/// This is deliberately not an NMP receipt. It exists only after the
/// configured threshold has been met by actual matching relay `OK true`
/// frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcknowledgedPublication {
    pub lease: PersistedEffectLeaseIdentity,
    pub publication_id: PublicationId,
    pub event_id_hex: String,
    pub required_acknowledgements: usize,
    pub relay_outcomes: Vec<RelayOutcome>,
}

impl AcknowledgedPublication {
    #[must_use]
    pub fn acknowledgement_count(&self) -> usize {
        self.relay_outcomes
            .iter()
            .filter(|outcome| outcome.is_acknowledged())
            .count()
    }
}

/// Non-receipt evidence retained on a failed publication call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationFailure {
    pub lease: PersistedEffectLeaseIdentity,
    pub publication_id: PublicationId,
    pub event_id_hex: Option<String>,
    pub required_acknowledgements: usize,
    pub relay_outcomes: Vec<RelayOutcome>,
}

impl PublicationFailure {
    #[must_use]
    pub fn acknowledgement_count(&self) -> usize {
        self.relay_outcomes
            .iter()
            .filter(|outcome| outcome.is_acknowledged())
            .count()
    }

    #[must_use]
    pub fn handoff_possible(&self) -> bool {
        self.relay_outcomes
            .iter()
            .any(RelayOutcome::handoff_possible)
    }
}
