use std::{fmt, net::IpAddr};

use reqwest::Url;

use crate::{InvalidRequestError, InvalidRequestReason, TtsError, error::InvalidRequestField};

#[derive(Clone)]
pub struct ElevenLabsEndpoint {
    base_url: Url,
}

impl ElevenLabsEndpoint {
    /// Creates a remote endpoint and requires HTTPS.
    pub fn new(value: &str) -> Result<Self, TtsError> {
        Self::parse(value, EndpointTransport::Https)
    }

    /// Explicitly opts a development/test endpoint into HTTP on a numeric loopback address.
    pub fn new_loopback_http(value: &str) -> Result<Self, TtsError> {
        Self::parse(value, EndpointTransport::LoopbackHttp)
    }

    fn parse(value: &str, transport: EndpointTransport) -> Result<Self, TtsError> {
        let mut base_url = Url::parse(value).map_err(|_| invalid_endpoint())?;
        if base_url.cannot_be_a_base()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || !transport.allows(&base_url)
        {
            return Err(invalid_endpoint());
        }
        base_url.set_query(None);
        base_url.set_fragment(None);
        Ok(Self { base_url })
    }

    pub(crate) fn generation_url(
        &self,
        voice_id: &str,
        output_format: Option<&str>,
    ) -> Result<Url, TtsError> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|_| invalid_endpoint())?;
            segments
                .pop_if_empty()
                .push("v1")
                .push("text-to-speech")
                .push(voice_id)
                .push("stream");
        }
        if let Some(format) = output_format {
            url.query_pairs_mut().append_pair("output_format", format);
        }
        Ok(url)
    }
}

#[derive(Clone, Copy)]
enum EndpointTransport {
    Https,
    LoopbackHttp,
}

impl EndpointTransport {
    fn allows(self, url: &Url) -> bool {
        match self {
            Self::Https => url.scheme() == "https",
            Self::LoopbackHttp => {
                url.scheme() == "http"
                    && url.host_str().is_some_and(|host| {
                        host.trim_start_matches('[')
                            .trim_end_matches(']')
                            .parse::<IpAddr>()
                            .is_ok_and(|address| address.is_loopback())
                    })
            }
        }
    }
}

impl fmt::Debug for ElevenLabsEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ElevenLabsEndpoint")
            .field(&self.base_url.as_str())
            .finish()
    }
}

fn invalid_endpoint() -> TtsError {
    TtsError::InvalidRequest(InvalidRequestError {
        field: InvalidRequestField::Endpoint,
        reason: InvalidRequestReason::Invalid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_appends_protocol_path_and_encodes_voice() {
        let endpoint = ElevenLabsEndpoint::new("https://example.test/proxy").unwrap();
        let url = endpoint
            .generation_url("voice/id", Some("mp3_44100_128"))
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.test/proxy/v1/text-to-speech/voice%2Fid/stream?output_format=mp3_44100_128"
        );
    }

    #[test]
    fn endpoint_rejects_embedded_credentials_and_query() {
        assert!(ElevenLabsEndpoint::new("https://secret@example.test").is_err());
        assert!(ElevenLabsEndpoint::new("https://example.test?token=secret").is_err());
    }

    #[test]
    fn endpoint_requires_https_without_loopback_opt_in() {
        assert!(ElevenLabsEndpoint::new("http://example.test").is_err());
        assert!(ElevenLabsEndpoint::new("http://127.0.0.1:8080").is_err());
        assert!(ElevenLabsEndpoint::new_loopback_http("http://example.test").is_err());
        assert!(ElevenLabsEndpoint::new_loopback_http("http://127.0.0.1:8080").is_ok());
        assert!(ElevenLabsEndpoint::new_loopback_http("http://[::1]:8080").is_ok());
        assert!(ElevenLabsEndpoint::new_loopback_http("http://localhost:8080").is_err());
    }
}
