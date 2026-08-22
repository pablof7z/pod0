use std::{fmt, time::Duration};

use reqwest::{Client, redirect::Policy};
use tokio::time::Instant;

use crate::{
    CancellationToken, GenerationEvidence, GenerationRequest, InvalidRequestError,
    InvalidRequestField, InvalidRequestReason, NetworkError, NetworkErrorKind, TtsError, file,
    response, validation,
};

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub connect_timeout: Duration,
    pub user_agent: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(15),
            user_agent: format!("pod0-tts-host/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

#[derive(Clone)]
pub struct TtsClient {
    client: Client,
}

impl TtsClient {
    pub fn new(config: ClientConfig) -> Result<Self, TtsError> {
        if config.connect_timeout.is_zero() {
            return Err(TtsError::InvalidRequest(InvalidRequestError {
                field: InvalidRequestField::Timeout,
                reason: InvalidRequestReason::Zero,
            }));
        }
        let client = Client::builder()
            .connect_timeout(config.connect_timeout)
            .redirect(Policy::none())
            .user_agent(config.user_agent)
            .build()
            .map_err(|_| {
                TtsError::Network(NetworkError {
                    kind: NetworkErrorKind::BuildClient,
                })
            })?;
        Ok(Self { client })
    }

    #[tracing::instrument(
        skip(self, request, cancellation),
        fields(status = tracing::field::Empty, duration_ms = tracing::field::Empty, outcome = tracing::field::Empty)
    )]
    pub async fn generate(
        &self,
        request: &GenerationRequest<'_>,
        cancellation: &CancellationToken,
    ) -> Result<GenerationEvidence, TtsError> {
        let started = Instant::now();
        let result = self.generate_inner(request, cancellation).await;
        let span = tracing::Span::current();
        span.record(
            "duration_ms",
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        );
        match &result {
            Ok(evidence) => {
                span.record("status", evidence.provider.status);
                span.record("outcome", "ok");
            }
            Err(error) => {
                span.record("outcome", tts_error_kind(error));
            }
        }
        result
    }

    async fn generate_inner(
        &self,
        request: &GenerationRequest<'_>,
        cancellation: &CancellationToken,
    ) -> Result<GenerationEvidence, TtsError> {
        validation::validate(request)?;
        let control = GenerationControl::new(request.timeout, cancellation)?;
        let mut temporary = control.prepare(|| file::create(request.staged_path))?;
        let transfer = control
            .run(async { response::download(&self.client, request, temporary.file_mut()).await })
            .await;
        let downloaded = match transfer {
            Ok(downloaded) => downloaded,
            Err(error) => {
                return Err(control.cleanup(temporary, error).await);
            }
        };
        let committed = control.commit(|| file::commit(temporary, request.staged_path))?;
        file::finish(committed);
        Ok(GenerationEvidence {
            staged_path: request.staged_path.to_owned(),
            media_type: downloaded.media_type,
            byte_count: downloaded.byte_count,
            content_digest: downloaded.content_digest,
            provider: crate::ProviderResponseEvidence {
                status: downloaded.status,
                request_id: downloaded.request_id,
            },
        })
    }
}

struct GenerationControl<'a> {
    deadline: Instant,
    cancellation: &'a CancellationToken,
}

impl<'a> GenerationControl<'a> {
    fn new(timeout: Duration, cancellation: &'a CancellationToken) -> Result<Self, TtsError> {
        let deadline = Instant::now().checked_add(timeout).ok_or({
            TtsError::InvalidRequest(InvalidRequestError {
                field: InvalidRequestField::Timeout,
                reason: InvalidRequestReason::Invalid,
            })
        })?;
        Ok(Self {
            deadline,
            cancellation,
        })
    }

    async fn run<T>(
        &self,
        future: impl Future<Output = Result<T, TtsError>>,
    ) -> Result<T, TtsError> {
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(TtsError::Cancelled),
            _ = tokio::time::sleep_until(self.deadline) => Err(TtsError::Timeout),
            result = future => result,
        }
    }

    async fn cleanup(&self, temporary: file::TemporaryOutput, original: TtsError) -> TtsError {
        match self
            .run(async {
                file::discard(temporary).await;
                Ok(())
            })
            .await
        {
            Ok(()) => original,
            Err(TtsError::Cancelled) => TtsError::Cancelled,
            Err(TtsError::Timeout) => TtsError::Timeout,
            Err(_) => original,
        }
    }

    fn commit<T>(&self, operation: impl FnOnce() -> Result<T, TtsError>) -> Result<T, TtsError> {
        self.cancellation
            .run_if_active(|| {
                if Instant::now() >= self.deadline {
                    Err(TtsError::Timeout)
                } else {
                    operation()
                }
            })
            .unwrap_or(Err(TtsError::Cancelled))
    }

    fn prepare<T>(&self, operation: impl FnOnce() -> Result<T, TtsError>) -> Result<T, TtsError> {
        self.cancellation
            .run_if_active(|| {
                if Instant::now() >= self.deadline {
                    return Err(TtsError::Timeout);
                }
                let result = operation();
                if Instant::now() >= self.deadline {
                    drop(result);
                    Err(TtsError::Timeout)
                } else {
                    result
                }
            })
            .unwrap_or(Err(TtsError::Cancelled))
    }
}

/// A short, non-secret error label for the `outcome` span field — never the
/// request/response body or credential, since `TtsError`'s `Debug` never
/// carries those either.
fn tts_error_kind(error: &TtsError) -> &'static str {
    match error {
        TtsError::InvalidRequest(_) => "invalid_request",
        TtsError::Credential(_) => "credential",
        TtsError::Network(_) => "network",
        TtsError::Provider(_) => "provider_error",
        TtsError::Protocol(_) => "protocol",
        TtsError::Limit(_) => "limit",
        TtsError::File(_) => "file",
        TtsError::Timeout => "timeout",
        TtsError::Cancelled => "cancelled",
    }
}

impl Default for TtsClient {
    fn default() -> Self {
        Self::new(ClientConfig::default()).expect("default TTS client configuration is valid")
    }
}

impl fmt::Debug for TtsClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TtsClient")
    }
}
