use std::path::Path;

use futures_util::stream;
use reqwest::{Body, multipart::Part};
use tokio::{
    fs::File,
    io::{AsyncReadExt as _, Error},
};

use crate::{AdapterError, SizeError, SizeSubject, provider::invalid_provider_response};

pub(crate) async fn audio_part(path: &Path, limit: u64) -> Result<Part, AdapterError> {
    let file = open_audio(path, limit).await?;
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid_provider_response("transcription audio file"))?
        .to_string_lossy()
        .into_owned();
    let body = Body::wrap_stream(bounded_upload_stream(file, limit));
    Ok(Part::stream(body).file_name(file_name))
}

async fn open_audio(path: &Path, limit: u64) -> Result<File, AdapterError> {
    let file = File::open(path)
        .await
        .map_err(|error| AdapterError::from_io("open transcription audio", &error))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| AdapterError::from_io("inspect transcription audio", &error))?;
    if !metadata.is_file() {
        return Err(invalid_provider_response("transcription audio file"));
    }
    if metadata.len() > limit {
        return Err(AdapterError::Size(SizeError {
            subject: SizeSubject::Upload,
            limit,
            observed: Some(metadata.len()),
        }));
    }
    Ok(file)
}

fn bounded_upload_stream(
    file: File,
    limit: u64,
) -> impl futures_util::Stream<Item = Result<Vec<u8>, AdapterError>> + Send + 'static {
    stream::try_unfold((file, 0_u64), move |(mut file, sent)| async move {
        let capacity = limit.saturating_sub(sent).saturating_add(1).min(8_192);
        let mut buffer = vec![0; usize::try_from(capacity).unwrap_or(8_192)];
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|error: Error| AdapterError::from_io("read transcription audio", &error))?;
        if count == 0 {
            return Ok(None);
        }
        buffer.truncate(count);
        let observed = sent.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if observed > limit {
            return Err(AdapterError::Size(SizeError {
                subject: SizeSubject::Upload,
                limit,
                observed: Some(observed),
            }));
        }
        Ok(Some((buffer, (file, observed))))
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use futures_util::StreamExt as _;

    use super::*;

    static FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[tokio::test]
    async fn opened_handle_is_reused_and_growth_cannot_cross_bound() {
        let path = test_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"1234").unwrap();
        let file = open_audio(&path, 4).await.unwrap();
        let original_path = path.with_extension("original");
        fs::rename(&path, &original_path).unwrap();
        fs::write(&path, b"replacement-secret").unwrap();

        let stream = bounded_upload_stream(file, 4);
        futures_util::pin_mut!(stream);
        let mut uploaded = Vec::new();
        while let Some(chunk) = stream.next().await {
            uploaded.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(uploaded, b"1234");

        let file = open_audio(&path, 32).await.unwrap();
        fs::write(&path, vec![b'x'; 33]).unwrap();
        let stream = bounded_upload_stream(file, 32);
        futures_util::pin_mut!(stream);
        let mut sent = 0;
        let mut failure = None;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => sent += chunk.len(),
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        assert!(sent <= 32);
        assert!(matches!(failure, Some(AdapterError::Size(_))));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(original_path);
        let _ = fs::remove_dir(path.parent().unwrap());
    }

    fn test_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test-artifacts")
            .join(format!(
                "upload-{}-{}",
                std::process::id(),
                FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
            ))
    }
}
