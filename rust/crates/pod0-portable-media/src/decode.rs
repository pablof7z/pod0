use std::{fs::File, io::Cursor, path::PathBuf, sync::Arc, time::Duration};

use rodio::{Decoder, Source};

use crate::{MediaError, Result};

pub(crate) type AudioSource = Box<dyn Source<Item = f32> + Send>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: Option<Duration>,
}

#[derive(Clone)]
pub struct PreparedMedia {
    pub(crate) data: MediaData,
    metadata: MediaMetadata,
}

#[derive(Clone)]
pub(crate) enum MediaData {
    File(PathBuf),
    Memory(Arc<[u8]>),
}

impl std::fmt::Debug for PreparedMedia {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedMedia")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl PreparedMedia {
    pub(crate) fn from_file(path: PathBuf) -> Result<Self> {
        let data = MediaData::File(path);
        Self::from_data(data)
    }

    pub(crate) fn from_memory(bytes: Arc<[u8]>) -> Result<Self> {
        Self::from_data(MediaData::Memory(bytes))
    }

    fn from_data(data: MediaData) -> Result<Self> {
        let decoder = decoder_for(&data)?;
        let metadata = MediaMetadata {
            sample_rate: decoder.sample_rate(),
            channels: decoder.channels(),
            duration: decoder.total_duration(),
        };
        Ok(Self { data, metadata })
    }

    #[must_use]
    pub fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    pub(crate) fn decoder(&self) -> Result<AudioSource> {
        decoder_for(&self.data)
    }
}

fn decoder_for(data: &MediaData) -> Result<AudioSource> {
    match data {
        MediaData::File(path) => {
            let file = File::open(path)?;
            let decoder =
                Decoder::try_from(file).map_err(|error| MediaError::Decode(error.to_string()))?;
            Ok(Box::new(decoder))
        }
        MediaData::Memory(bytes) => {
            let cursor = Cursor::new(Arc::clone(bytes));
            let decoder = Decoder::builder()
                .with_data(cursor)
                .with_byte_len(bytes.len() as u64)
                .build()
                .map_err(|error| MediaError::Decode(error.to_string()))?;
            Ok(Box::new(decoder))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };

    use rodio::Source;

    use super::PreparedMedia;

    const SAMPLE_RATE: u32 = 8_000;

    #[test]
    fn file_and_memory_decoders_preserve_seekability_and_duration() {
        let path = test_path("seekable.wav");
        write_wav(&path, 2_000);
        let bytes = fs::read(&path).expect("read generated WAV");

        for media in [
            PreparedMedia::from_file(path.clone()).expect("prepare file"),
            PreparedMedia::from_memory(Arc::from(bytes)).expect("prepare memory"),
        ] {
            assert_eq!(media.metadata().duration, Some(Duration::from_secs(2)));
            let mut decoder = media.decoder().expect("create decoder");
            decoder
                .try_seek(Duration::from_millis(1_500))
                .expect("known-length decoder supports seeking");
            assert!(decoder.next().is_some());
        }

        fs::remove_file(&path).expect("remove generated WAV");
        fs::remove_dir(path.parent().expect("test directory")).expect("remove test directory");
    }

    fn write_wav(path: &Path, duration_milliseconds: u64) {
        let specification = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, specification).expect("create WAV");
        let samples = duration_milliseconds * u64::from(SAMPLE_RATE) / 1_000;
        for sample in 0..samples {
            writer
                .write_sample((sample % u64::from(i16::MAX as u16)) as i16)
                .expect("write WAV sample");
        }
        writer.finalize().expect("finalize WAV");
    }

    fn test_path(name: &str) -> PathBuf {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("decode-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("create test directory");
        directory.join(name)
    }
}
