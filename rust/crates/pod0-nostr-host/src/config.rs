use std::{collections::BTreeSet, time::Duration};

use nostr::RelayUrl;
use pod0_domain::PublicationRouteId;
use sha2::{Digest as _, Sha256};
use url::{Host, Url};

use crate::ConfigurationError;

const MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_AUTHENTICATION_CHALLENGES: usize = 32;

/// Required relay acknowledgement count.
///
/// `All` requires every configured distinct relay. `AtLeast(n)` stops after
/// the first `n` actual positive NIP-01 acknowledgements. Configuration rejects
/// zero or a value larger than the relay count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcknowledgementThreshold {
    All,
    AtLeast(usize),
}

impl AcknowledgementThreshold {
    pub(crate) fn required(self, relay_count: usize) -> usize {
        match self {
            Self::All => relay_count,
            Self::AtLeast(count) => count,
        }
    }
}

/// Bounded transport policy owned by the platform executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherConfig {
    pub acknowledgement_threshold: AcknowledgementThreshold,
    pub operation_timeout: Duration,
    pub connect_timeout: Duration,
    pub acknowledgement_timeout: Duration,
    pub cancellation_poll_interval: Duration,
    pub maximum_incoming_message_bytes: usize,
    pub maximum_authentication_challenges: usize,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            acknowledgement_threshold: AcknowledgementThreshold::All,
            operation_timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(10),
            acknowledgement_timeout: Duration::from_secs(15),
            cancellation_poll_interval: Duration::from_millis(100),
            maximum_incoming_message_bytes: 128 * 1024,
            maximum_authentication_challenges: 4,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RelaySecurity {
    // Stricter policy than `NostrPublisher::new` currently selects; exercised
    // directly by this module's own unit tests, no production caller yet.
    #[allow(dead_code)]
    SecureOnly,
    AllowInsecureNumericLoopback,
}

#[derive(Clone, Debug)]
pub(crate) struct RelayTarget {
    pub url: RelayUrl,
    pub request_url: String,
    pub host: String,
    pub port: u16,
    pub route_id: PublicationRouteId,
}

pub(crate) fn validated_targets<I, S>(
    relay_urls: I,
    config: &PublisherConfig,
    security: RelaySecurity,
) -> Result<(Vec<RelayTarget>, usize), ConfigurationError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    validate_durations(config)?;
    let mut seen_origins = BTreeSet::new();
    let mut targets = Vec::new();
    for (index, supplied) in relay_urls.into_iter().enumerate() {
        let parsed = Url::parse(supplied.as_ref())
            .map_err(|_| ConfigurationError::InvalidRelayUrl { index })?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(ConfigurationError::CredentialedRelayUrl { index });
        }
        if parsed.fragment().is_some() {
            return Err(ConfigurationError::FragmentedRelayUrl { index });
        }
        validate_relay_security(&parsed, index, security)?;
        let relay = RelayUrl::parse(parsed.as_str())
            .map_err(|_| ConfigurationError::InvalidRelayUrl { index })?;
        let origin = parsed.origin().ascii_serialization();
        if !seen_origins.insert(origin) {
            return Err(ConfigurationError::DuplicateRelayOrigin { index });
        }
        let host = parsed
            .host_str()
            .ok_or(ConfigurationError::InvalidRelayUrl { index })?
            .to_owned();
        let port = parsed
            .port_or_known_default()
            .ok_or(ConfigurationError::InvalidRelayUrl { index })?;
        let request_url = parsed.to_string();
        targets.push(RelayTarget {
            route_id: route_id(&request_url),
            url: relay,
            request_url,
            host,
            port,
        });
    }
    if targets.is_empty() {
        return Err(ConfigurationError::NoRelayUrls);
    }
    let required = config.acknowledgement_threshold.required(targets.len());
    if required == 0 {
        return Err(ConfigurationError::ZeroAcknowledgementThreshold);
    }
    if required > targets.len() {
        return Err(ConfigurationError::UnreachableAcknowledgementThreshold {
            required,
            relay_count: targets.len(),
        });
    }
    Ok((targets, required))
}

fn validate_durations(config: &PublisherConfig) -> Result<(), ConfigurationError> {
    for (name, duration) in [
        ("operation_timeout", config.operation_timeout),
        ("connect_timeout", config.connect_timeout),
        ("acknowledgement_timeout", config.acknowledgement_timeout),
        (
            "cancellation_poll_interval",
            config.cancellation_poll_interval,
        ),
    ] {
        if duration.is_zero() || duration > MAX_TIMEOUT {
            return Err(ConfigurationError::InvalidDuration { name });
        }
    }
    if config.maximum_incoming_message_bytes == 0 {
        return Err(ConfigurationError::InvalidIncomingMessageLimit);
    }
    if !(1..=MAX_AUTHENTICATION_CHALLENGES)
        .contains(&config.maximum_authentication_challenges)
    {
        return Err(ConfigurationError::InvalidAuthenticationChallengeLimit);
    }
    Ok(())
}

fn validate_relay_security(
    parsed: &Url,
    index: usize,
    security: RelaySecurity,
) -> Result<(), ConfigurationError> {
    match parsed.scheme() {
        "wss" => Ok(()),
        "ws" if matches!(security, RelaySecurity::SecureOnly) => {
            Err(ConfigurationError::InsecureRelayUrl { index })
        }
        "ws" => match parsed.host() {
            Some(Host::Ipv4(address)) if address.is_loopback() => Ok(()),
            Some(Host::Ipv6(address)) if address.is_loopback() => Ok(()),
            _ => Err(ConfigurationError::NonLoopbackInsecureRelayUrl { index }),
        },
        _ => Err(ConfigurationError::InvalidRelayUrl { index }),
    }
}

fn route_id(url: &str) -> PublicationRouteId {
    let mut hash = Sha256::new();
    hash.update(b"pod0.nostr-host.relay-route.v1\0");
    hash.update(url.as_bytes());
    PublicationRouteId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_and_relay_identity_are_fail_closed() {
        let config = PublisherConfig {
            acknowledgement_threshold: AcknowledgementThreshold::AtLeast(2),
            ..PublisherConfig::default()
        };
        let error =
            validated_targets(["wss://relay.example"], &config, RelaySecurity::SecureOnly)
                .unwrap_err();
        assert!(matches!(
            error,
            ConfigurationError::UnreachableAcknowledgementThreshold { .. }
        ));

        let error = validated_targets(
            [
                "wss://relay.example/path?first=1",
                "wss://RELAY.EXAMPLE:443/other?second=2",
            ],
            &PublisherConfig::default(),
            RelaySecurity::SecureOnly,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ConfigurationError::DuplicateRelayOrigin { .. }
        ));
    }

    #[test]
    fn insecure_relays_require_the_explicit_numeric_loopback_policy() {
        let error = validated_targets(
            ["ws://127.0.0.1:8080"],
            &PublisherConfig::default(),
            RelaySecurity::SecureOnly,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ConfigurationError::InsecureRelayUrl { .. }
        ));

        validated_targets(
            ["ws://127.0.0.1:8080/path"],
            &PublisherConfig::default(),
            RelaySecurity::AllowInsecureNumericLoopback,
        )
        .unwrap();
        let error = validated_targets(
            ["ws://localhost:8080"],
            &PublisherConfig::default(),
            RelaySecurity::AllowInsecureNumericLoopback,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ConfigurationError::NonLoopbackInsecureRelayUrl { .. }
        ));
    }
}
