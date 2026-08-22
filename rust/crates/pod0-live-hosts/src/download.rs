use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use futures_util::StreamExt as _;
use tokio::io::AsyncWriteExt as _;

use crate::{
    AdapterError, CancellationToken, HttpEvidence, LiveHosts, ProtocolError, RequestOptions,
    SizeError, SizeSubject, UnavailableError,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub url: String,
    pub staged_path: PathBuf,
    pub accept: Option<String>,
    pub entity_tag: Option<String>,
    pub last_modified: Option<String>,
    pub options: RequestOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadResponse {
    pub staged_path: PathBuf,
    pub byte_count: u64,
    pub evidence: HttpEvidence,
}

impl LiveHosts {
    pub async fn download(
        &self,
        request: DownloadRequest,
        cancellation: &CancellationToken,
    ) -> Result<DownloadResponse, AdapterError> {
        cancellation.check()?;
        request.options.limits.validate()?;
        let temp_path = temporary_path(&request.staged_path)?;
        let result = self
            .run(request.options.timeout, cancellation, async {
                self.download_inner(&request, &temp_path).await
            })
            .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp_path).await;
        }
        result
    }

    async fn download_inner(
        &self,
        request: &DownloadRequest,
        temp_path: &Path,
    ) -> Result<DownloadResponse, AdapterError> {
        let (response, evidence) = self
            .follow_get(
                &request.url,
                request.accept.as_deref(),
                request.entity_tag.as_deref(),
                request.last_modified.as_deref(),
                request.options,
            )
            .await?;
        match evidence.status {
            200 => {}
            404 | 410 => {
                return Err(AdapterError::Unavailable(UnavailableError {
                    capability: "download",
                    status: Some(evidence.status),
                }));
            }
            status => {
                return Err(AdapterError::Protocol(ProtocolError {
                    context: "download HTTP status",
                    status: Some(status),
                    evidence: Some(Box::new(evidence)),
                }));
            }
        }
        if evidence
            .content_length
            .is_some_and(|length| length > request.options.limits.maximum_body_bytes)
        {
            return Err(AdapterError::Size(SizeError {
                subject: SizeSubject::ResponseBody,
                limit: request.options.limits.maximum_body_bytes,
                observed: evidence.content_length,
            }));
        }
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp_path)
            .await
            .map_err(|error| AdapterError::from_io("create temporary download", &error))?;
        let mut byte_count = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| AdapterError::response_body())?;
            byte_count = byte_count
                .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
                .ok_or(AdapterError::Size(SizeError {
                    subject: SizeSubject::ResponseBody,
                    limit: request.options.limits.maximum_body_bytes,
                    observed: None,
                }))?;
            if byte_count > request.options.limits.maximum_body_bytes {
                return Err(AdapterError::Size(SizeError {
                    subject: SizeSubject::ResponseBody,
                    limit: request.options.limits.maximum_body_bytes,
                    observed: Some(byte_count),
                }));
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| AdapterError::from_io("write temporary download", &error))?;
        }
        file.flush()
            .await
            .map_err(|error| AdapterError::from_io("flush temporary download", &error))?;
        file.sync_all()
            .await
            .map_err(|error| AdapterError::from_io("sync temporary download", &error))?;
        drop(file);
        finalize_download(temp_path, &request.staged_path).await?;
        Ok(DownloadResponse {
            staged_path: request.staged_path.clone(),
            byte_count,
            evidence,
        })
    }
}

async fn finalize_download(temp_path: &Path, staged_path: &Path) -> Result<(), AdapterError> {
    tokio::fs::hard_link(temp_path, staged_path)
        .await
        .map_err(|error| AdapterError::from_io("finalize staged download", &error))?;
    tokio::fs::remove_file(temp_path)
        .await
        .map_err(|error| AdapterError::from_io("remove temporary download", &error))
}

fn temporary_path(staged_path: &Path) -> Result<PathBuf, AdapterError> {
    let parent = staged_path
        .parent()
        .ok_or(AdapterError::Protocol(ProtocolError {
            context: "staged download path",
            status: None,
            evidence: None,
        }))?;
    let file_name = staged_path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(AdapterError::Protocol(ProtocolError {
            context: "staged download path",
            status: None,
            evidence: None,
        }))?;
    let mut temporary = OsString::from(".");
    temporary.push(file_name);
    temporary.push(format!(
        ".pod0-part-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(parent.join(temporary))
}
