use std::time::Duration;

use reqwest::{Client, Url, redirect::Policy};

use crate::{AdapterError, CancellationToken, NetworkError, NetworkErrorKind, ProtocolError};

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub connect_timeout: Duration,
    pub user_agent: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(15),
            user_agent: format!("pod0-live-hosts/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LiveHosts {
    pub(crate) client: Client,
}

impl LiveHosts {
    pub fn new(config: ClientConfig) -> Result<Self, AdapterError> {
        let client = Client::builder()
            .connect_timeout(config.connect_timeout)
            .redirect(Policy::none())
            .user_agent(config.user_agent)
            .build()
            .map_err(|error| AdapterError::from_reqwest(&error))?;
        Ok(Self { client })
    }

    pub(crate) async fn run<T>(
        &self,
        timeout: Duration,
        cancellation: &CancellationToken,
        future: impl Future<Output = Result<T, AdapterError>>,
    ) -> Result<T, AdapterError> {
        cancellation.check()?;
        if timeout.is_zero() {
            return Err(AdapterError::Network(NetworkError {
                kind: NetworkErrorKind::Timeout,
            }));
        }
        tokio::select! {
            _ = cancellation.cancelled() => Err(AdapterError::Cancelled),
            result = tokio::time::timeout(timeout, future) => match result {
                Ok(result) => result,
                Err(_) => Err(AdapterError::Network(NetworkError {
                    kind: NetworkErrorKind::Timeout,
                })),
            },
        }
    }

    pub(crate) fn parse_url(
        &self,
        value: &str,
        context: &'static str,
    ) -> Result<Url, AdapterError> {
        let url = Url::parse(value).map_err(|_| {
            AdapterError::Protocol(ProtocolError {
                context,
                status: None,
                evidence: None,
            })
        })?;
        self.validate_scheme(&url, context)?;
        Ok(url)
    }

    pub(crate) fn validate_scheme(
        &self,
        url: &Url,
        context: &'static str,
    ) -> Result<(), AdapterError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(AdapterError::Protocol(ProtocolError {
                context,
                status: None,
                evidence: None,
            }));
        }
        Ok(())
    }
}

impl Default for LiveHosts {
    fn default() -> Self {
        Self::new(ClientConfig::default()).expect("default HTTP client configuration is valid")
    }
}
