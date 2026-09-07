use std::{
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use rodio::Source;

use crate::{CancellationToken, MediaError, PreparedMedia, Result, publish::publish_file};

static PARTIAL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MAX_PARTIAL_CREATE_ATTEMPTS: u64 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipEncoding {
    /// RIFF/WAVE, interleaved signed 16-bit little-endian PCM.
    WavPcmSigned16LittleEndian,
}

#[derive(Clone, Debug)]
pub struct ClipExportRequest {
    pub input: PathBuf,
    pub output: PathBuf,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub overwrite: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipExport {
    pub output: PathBuf,
    pub encoding: ClipEncoding,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub duration_milliseconds: u64,
    pub file_size_bytes: u64,
}

pub fn export_clip(
    request: &ClipExportRequest,
    cancellation: &CancellationToken,
) -> Result<ClipExport> {
    validate_request(request)?;
    cancellation.check()?;

    let media = PreparedMedia::from_file(request.input.clone())?;
    let metadata = media.metadata();
    if let Some(duration) = metadata.duration
        && Duration::from_millis(request.end_milliseconds) > duration
    {
        return Err(MediaError::ClipEndOutOfRange {
            end_milliseconds: request.end_milliseconds,
            duration_milliseconds: duration.as_millis() as u64,
        });
    }

    let start_frame = milliseconds_to_frame(request.start_milliseconds, metadata.sample_rate);
    let end_frame = milliseconds_to_frame(request.end_milliseconds, metadata.sample_rate);
    let expected_frames = end_frame.saturating_sub(start_frame);
    let (mut partial, partial_file) = PartialOutput::create(&request.output)?;
    let frames = encode_clip(
        &media,
        partial_file,
        start_frame,
        expected_frames,
        cancellation,
    )?;
    verify_wav(
        partial.path(),
        metadata.sample_rate,
        metadata.channels,
        frames,
    )?;
    cancellation.check()?;
    publish_file(partial.path(), &request.output, request.overwrite)?;
    partial.published = true;

    let file_size_bytes = fs::metadata(&request.output)?.len();
    Ok(ClipExport {
        output: request.output.clone(),
        encoding: ClipEncoding::WavPcmSigned16LittleEndian,
        sample_rate: metadata.sample_rate,
        channels: metadata.channels,
        frames,
        duration_milliseconds: frames.saturating_mul(1_000) / u64::from(metadata.sample_rate),
        file_size_bytes,
    })
}

fn validate_request(request: &ClipExportRequest) -> Result<()> {
    if request.start_milliseconds >= request.end_milliseconds {
        return Err(MediaError::InvalidClipBounds {
            start_milliseconds: request.start_milliseconds,
            end_milliseconds: request.end_milliseconds,
        });
    }
    if paths_refer_to_same_file(&request.input, &request.output)? {
        return Err(MediaError::ClipInputEqualsOutput);
    }
    if request.output.exists() && !request.overwrite {
        return Err(MediaError::OutputExists(request.output.clone()));
    }
    if let Some(parent) = request.output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn encode_clip(
    media: &PreparedMedia,
    output: File,
    start_frame: u64,
    expected_frames: u64,
    cancellation: &CancellationToken,
) -> Result<u64> {
    let metadata = media.metadata();
    let mut decoder = media.decoder()?;
    let seek_position = Duration::from_secs_f64(start_frame as f64 / metadata.sample_rate as f64);
    if decoder.try_seek(seek_position).is_err() {
        decoder = media.decoder()?;
        discard_frames(&mut decoder, start_frame, metadata.channels, cancellation)?;
    }

    let specification = hound::WavSpec {
        channels: metadata.channels,
        sample_rate: metadata.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(output, specification)?;
    let expected_samples = expected_frames.saturating_mul(u64::from(metadata.channels));
    let mut written = 0_u64;
    while written < expected_samples {
        if written.is_multiple_of(16_384) {
            cancellation.check()?;
        }
        let Some(sample) = decoder.next() else {
            break;
        };
        writer.write_sample(sample_to_i16(sample))?;
        written += 1;
    }
    writer.finalize()?;
    let frames = written / u64::from(metadata.channels);
    if frames != expected_frames {
        return Err(MediaError::TruncatedClip {
            actual_frames: frames,
            expected_frames,
        });
    }
    Ok(frames)
}

fn discard_frames(
    decoder: &mut Box<dyn Source<Item = f32> + Send>,
    frames: u64,
    channels: u16,
    cancellation: &CancellationToken,
) -> Result<()> {
    let samples = frames.saturating_mul(u64::from(channels));
    for index in 0..samples {
        if index.is_multiple_of(16_384) {
            cancellation.check()?;
        }
        if decoder.next().is_none() {
            return Err(MediaError::TruncatedClip {
                actual_frames: index / u64::from(channels),
                expected_frames: frames,
            });
        }
    }
    Ok(())
}

fn verify_wav(
    path: &Path,
    expected_sample_rate: u32,
    expected_channels: u16,
    expected_frames: u64,
) -> Result<()> {
    let reader = hound::WavReader::open(path)?;
    let specification = reader.spec();
    let frames = u64::from(reader.duration());
    if specification.sample_rate != expected_sample_rate
        || specification.channels != expected_channels
        || specification.bits_per_sample != 16
        || specification.sample_format != hound::SampleFormat::Int
        || frames != expected_frames
    {
        return Err(MediaError::ClipVerification(format!(
            "expected {expected_channels} channels at {expected_sample_rate} Hz and \
             {expected_frames} frames of PCM16; found {} channels at {} Hz and {frames} frames",
            specification.channels, specification.sample_rate
        )));
    }
    if fs::metadata(path)?.len() <= 44 {
        return Err(MediaError::ClipVerification(
            "encoded WAV has no audio payload".to_owned(),
        ));
    }
    Ok(())
}

fn milliseconds_to_frame(milliseconds: u64, sample_rate: u32) -> u64 {
    u128::from(milliseconds)
        .saturating_mul(u128::from(sample_rate))
        .checked_div(1_000)
        .unwrap_or_default()
        .min(u128::from(u64::MAX)) as u64
}

fn sample_to_i16(sample: f32) -> i16 {
    if sample <= -1.0 {
        i16::MIN
    } else if sample >= 1.0 {
        i16::MAX
    } else {
        (sample * 32_768.0).round() as i16
    }
}

fn partial_path(output: &Path, sequence: u64) -> PathBuf {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("clip.wav");
    output.with_file_name(format!(".{name}.partial-{}-{sequence}", std::process::id()))
}

fn paths_refer_to_same_file(input: &Path, output: &Path) -> Result<bool> {
    if input == output {
        return Ok(true);
    }
    if !output.exists() {
        return Ok(false);
    }
    Ok(fs::canonicalize(input)? == fs::canonicalize(output)?)
}

struct PartialOutput {
    path: PathBuf,
    published: bool,
}

impl PartialOutput {
    fn create(output: &Path) -> Result<(Self, File)> {
        let first_sequence = PARTIAL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        for attempt in 0..MAX_PARTIAL_CREATE_ATTEMPTS {
            let path = partial_path(output, first_sequence.wrapping_add(attempt));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok((
                        Self {
                            path,
                            published: false,
                        },
                        file,
                    ));
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }

        Err(io::Error::new(
            ErrorKind::AlreadyExists,
            format!(
                "could not create a unique partial file for {} after \
                 {MAX_PARTIAL_CREATE_ATTEMPTS} attempts",
                output.display()
            ),
        )
        .into())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PartialOutput {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}
