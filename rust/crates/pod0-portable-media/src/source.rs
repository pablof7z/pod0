use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use reqwest::Client;
use tokio::runtime::{Handle, Runtime};
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

enum RuntimeSource {
    Owned,
    Shared(Handle),
}

impl MediaLoader {
    pub fn new(options: HttpLoadOptions) -> Result<Self> {
        Self::from_options(options, RuntimeSource::Owned)
    }

    /// Like [`MediaLoader::new`], but drives its HTTP calls on the caller's
    /// existing `Handle` instead of building a second `Runtime` — used when a
    /// process (e.g. `pod0-cli`) already owns the one `Runtime` for the whole
    /// process and must avoid a colliding second one.
    pub fn new_with_handle(options: HttpLoadOptions, runtime_handle: Handle) -> Result<Self> {
        Self::from_options(options, RuntimeSource::Shared(runtime_handle))
    }

    fn from_options(options: HttpLoadOptions, runtime_source: RuntimeSource) -> Result<Self> {
        let maximum_response_bytes = options.maximum_response_bytes;
        let client_builder = Client::builder()
            .connect_timeout(options.connect_timeout)
            .timeout(options.request_timeout)
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(options.user_agent);
        Self::from_client_builder(client_builder, maximum_response_bytes, runtime_source)
    }

    fn from_client_builder(
        client_builder: reqwest::ClientBuilder,
        maximum_response_bytes: u64,
        runtime_source: RuntimeSource,
    ) -> Result<Self> {
        let client = client_builder.build()?;
        let (owned_runtime, handle) = match runtime_source {
            RuntimeSource::Owned => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                let handle = runtime.handle().clone();
                (Some(runtime), handle)
            }
            RuntimeSource::Shared(handle) => (None, handle),
        };
        Ok(Self {
            http: Arc::new(HttpTransport {
                client,
                owned_runtime,
                handle,
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
        // When this transport owns its `Runtime` (the `MediaLoader::new` path),
        // drive it via `Runtime::block_on` directly rather than through a
        // cloned `Handle` — a current-thread runtime's I/O/timer driver only
        // makes progress on the thread driving it, and `Runtime::block_on`
        // (unlike `Handle::block_on`) works correctly regardless of which
        // thread calls it. Only the caller-owned-runtime path (`new_with_handle`)
        // uses the `Handle`, and it is that caller's responsibility to ensure
        // its runtime supports being driven by `Handle::block_on` from other
        // threads (e.g. by using a multi-thread runtime).
        match &self.http.owned_runtime {
            Some(runtime) => runtime.block_on(self.load_http_async(url, cancellation)),
            None => self
                .http
                .handle
                .block_on(self.load_http_async(url, cancellation)),
        }
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
    // `Some` only when this transport built its own `Runtime` (`MediaLoader::new`);
    // `None` when driven by a caller-owned `Handle` (`MediaLoader::new_with_handle`),
    // in which case the caller is responsible for that runtime's lifecycle.
    owned_runtime: Option<Runtime>,
    handle: Handle,
}

impl Drop for HttpTransport {
    fn drop(&mut self) {
        if let Some(runtime) = self.owned_runtime.take() {
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
