use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use reqwest::Client;
use tokio::runtime::Runtime;
use url::Url;

use crate::{CancellationToken, MediaError, PreparedMedia, Result};

const DEFAULT_MAXIMUM_HTTP_BYTES: u64 = 512 * 1024 * 1024;
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaSource {
    File(PathBuf),
    Http(Url),
}

impl MediaSource {
    pub fn from_location(location: impl AsRef<str>) -> Result<Self> {
        let location = location.as_ref();
        if let Ok(url) = Url::parse(location) {
            return match url.scheme() {
                "http" | "https" => Ok(Self::Http(url)),
                "file" => url
                    .to_file_path()
                    .map(Self::File)
                    .map_err(|()| MediaError::InvalidLocalFile(PathBuf::from(location))),
                scheme => Err(MediaError::UnsupportedScheme(scheme.to_owned())),
            };
        }
        Ok(Self::File(PathBuf::from(location)))
    }
}

impl From<PathBuf> for MediaSource {
    fn from(value: PathBuf) -> Self {
        Self::File(value)
    }
}

impl From<&Path> for MediaSource {
    fn from(value: &Path) -> Self {
        Self::File(value.to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct HttpLoadOptions {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub maximum_response_bytes: u64,
    pub user_agent: String,
}

impl Default for HttpLoadOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(120),
            maximum_response_bytes: DEFAULT_MAXIMUM_HTTP_BYTES,
            user_agent: format!("pod0-portable-media/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MediaLoader {
    http: Arc<HttpTransport>,
    maximum_response_bytes: u64,
}

impl MediaLoader {
    pub fn new(options: HttpLoadOptions) -> Result<Self> {
        let maximum_response_bytes = options.maximum_response_bytes;
        let client_builder = Client::builder()
            .connect_timeout(options.connect_timeout)
            .timeout(options.request_timeout)
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(options.user_agent);
        Self::from_client_builder(client_builder, maximum_response_bytes)
    }

    fn from_client_builder(
        client_builder: reqwest::ClientBuilder,
        maximum_response_bytes: u64,
    ) -> Result<Self> {
        let client = client_builder.build()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(Self {
            http: Arc::new(HttpTransport {
                client,
                runtime: Some(runtime),
            }),
            maximum_response_bytes,
        })
    }

    pub fn load(
        &self,
        source: impl Into<MediaSource>,
        cancellation: &CancellationToken,
    ) -> Result<PreparedMedia> {
        cancellation.check()?;
        match source.into() {
            MediaSource::File(path) => load_file(path),
            MediaSource::Http(url) => self.load_http(url, cancellation),
        }
    }

    fn load_http(&self, url: Url, cancellation: &CancellationToken) -> Result<PreparedMedia> {
        self.http
            .runtime()
            .block_on(self.load_http_async(url, cancellation))
    }

    async fn load_http_async(
        &self,
        url: Url,
        cancellation: &CancellationToken,
    ) -> Result<PreparedMedia> {
        let response = await_http(self.http.client.get(url).send(), cancellation).await?;
        let mut response = response.error_for_status()?;
        if let Some(declared) = response.content_length()
            && declared > self.maximum_response_bytes
        {
            return Err(MediaError::HttpContentTooLarge {
                declared,
                maximum: self.maximum_response_bytes,
            });
        }

        let mut bytes = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or_default()
                .min(self.maximum_response_bytes) as usize,
        );
        while let Some(chunk) = await_http(response.chunk(), cancellation).await? {
            if bytes.len().saturating_add(chunk.len()) as u64 > self.maximum_response_bytes {
                return Err(MediaError::HttpBodyTooLarge {
                    maximum: self.maximum_response_bytes,
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        cancellation.check()?;
        PreparedMedia::from_memory(Arc::from(bytes))
    }
}

#[derive(Debug)]
struct HttpTransport {
    client: Client,
    runtime: Option<Runtime>,
}

impl HttpTransport {
    fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("HTTP runtime is available until transport drop")
    }
}

impl Drop for HttpTransport {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            // System DNS may still be inside Tokio's unabortable spawn_blocking.
            runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
        }
    }
}

async fn await_http<F, T>(future: F, cancellation: &CancellationToken) -> Result<T>
where
    F: Future<Output = std::result::Result<T, reqwest::Error>>,
{
    let mut future = Box::pin(future);
    loop {
        cancellation.check()?;
        match tokio::time::timeout(CANCELLATION_POLL_INTERVAL, future.as_mut()).await {
            Ok(result) => return Ok(result?),
            Err(_) => continue,
        }
    }
}

fn load_file(path: PathBuf) -> Result<PreparedMedia> {
    let metadata = fs::metadata(&path).map_err(|_| MediaError::InvalidLocalFile(path.clone()))?;
    if !metadata.is_file() {
        return Err(MediaError::InvalidLocalFile(path));
    }
    PreparedMedia::from_file(path)
}

#[cfg(test)]
mod tests;
