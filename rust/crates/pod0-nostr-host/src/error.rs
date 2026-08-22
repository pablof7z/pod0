use std::fmt;

use crate::PublicationFailure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SigningSecretError;

impl fmt::Display for SigningSecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Nostr signing secret")
    }
}

impl std::error::Error for SigningSecretError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigurationError {
    NoRelayUrls,
    InvalidRelayUrl { index: usize },
    CredentialedRelayUrl { index: usize },
    FragmentedRelayUrl { index: usize },
    InsecureRelayUrl { index: usize },
    NonLoopbackInsecureRelayUrl { index: usize },
    DuplicateRelayOrigin { index: usize },
    ZeroAcknowledgementThreshold,
    UnreachableAcknowledgementThreshold { required: usize, relay_count: usize },
    InvalidDuration { name: &'static str },
    InvalidIncomingMessageLimit,
    InvalidAuthenticationChallengeLimit,
    RuntimeInitializationFailed,
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRelayUrls => formatter.write_str("at least one relay URL is required"),
            Self::InvalidRelayUrl { index } => {
                write!(formatter, "relay URL at index {index} is invalid")
            }
            Self::CredentialedRelayUrl { index } => {
                write!(formatter, "relay URL at index {index} contains credentials")
            }
            Self::FragmentedRelayUrl { index } => {
                write!(formatter, "relay URL at index {index} contains a fragment")
            }
            Self::InsecureRelayUrl { index } => {
                write!(formatter, "relay URL at index {index} must use wss")
            }
            Self::NonLoopbackInsecureRelayUrl { index } => {
                write!(
                    formatter,
                    "insecure relay URL at index {index} must use a numeric loopback address"
                )
            }
            Self::DuplicateRelayOrigin { index } => {
                write!(
                    formatter,
                    "relay URL at index {index} duplicates a configured relay origin"
                )
            }
            Self::ZeroAcknowledgementThreshold => {
                formatter.write_str("acknowledgement threshold cannot be zero")
            }
            Self::UnreachableAcknowledgementThreshold {
                required,
                relay_count,
            } => write!(
                formatter,
                "acknowledgement threshold {required} exceeds {relay_count} relays"
            ),
            Self::InvalidDuration { name } => {
                write!(
                    formatter,
                    "{name} must be greater than zero and at most 24 hours"
                )
            }
            Self::InvalidIncomingMessageLimit => {
                formatter.write_str("incoming message limit must be nonzero")
            }
            Self::InvalidAuthenticationChallengeLimit => {
                formatter.write_str("authentication challenge limit must be between 1 and 32")
            }
            Self::RuntimeInitializationFailed => {
                formatter.write_str("async runtime initialization failed")
            }
        }
    }
}

impl std::error::Error for ConfigurationError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftError {
    InvalidExpectedAuthor,
    AuthorMismatch {
        expected_author_hex: String,
        configured_author_hex: String,
    },
    InvalidTag {
        index: usize,
    },
    SigningFailed,
    EventConstructionChangedDraft,
    EventSerializationFailed,
}

impl fmt::Display for DraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExpectedAuthor => {
                formatter.write_str("draft expected author is not lowercase 32-byte hex")
            }
            Self::AuthorMismatch { .. } => {
                formatter.write_str("configured signer does not match draft expected author")
            }
            Self::InvalidTag { index } => write!(formatter, "draft tag at index {index} is empty"),
            Self::SigningFailed => formatter.write_str("Nostr event signing failed"),
            Self::EventConstructionChangedDraft => {
                formatter.write_str("constructed event does not preserve the exact draft")
            }
            Self::EventSerializationFailed => {
                formatter.write_str("signed Nostr event serialization failed")
            }
        }
    }
}

impl std::error::Error for DraftError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishError {
    InvalidDraft(DraftError),
    Cancelled(Box<PublicationFailure>),
    LeaseExpired(Box<PublicationFailure>),
    OperationTimedOut(Box<PublicationFailure>),
    AcknowledgementThresholdNotMet(Box<PublicationFailure>),
}

impl PublishError {
    #[must_use]
    pub fn failure(&self) -> Option<&PublicationFailure> {
        match self {
            Self::InvalidDraft(_) => None,
            Self::Cancelled(failure)
            | Self::LeaseExpired(failure)
            | Self::OperationTimedOut(failure)
            | Self::AcknowledgementThresholdNotMet(failure) => Some(failure),
        }
    }
}

impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDraft(error) => error.fmt(formatter),
            Self::Cancelled(_) => formatter.write_str("publication was cancelled"),
            Self::LeaseExpired(_) => formatter.write_str("publication lease expired"),
            Self::OperationTimedOut(_) => formatter.write_str("publication operation timed out"),
            Self::AcknowledgementThresholdNotMet(_) => {
                formatter.write_str("relay acknowledgement threshold was not met")
            }
        }
    }
}

impl std::error::Error for PublishError {}
