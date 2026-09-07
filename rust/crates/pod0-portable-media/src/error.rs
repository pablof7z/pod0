use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("operation was cancelled")]
    Cancelled,
    #[error("media source uses unsupported URL scheme `{0}`")]
    UnsupportedScheme(String),
    #[error("media file does not exist or is not a regular file: {}", .0.display())]
    InvalidLocalFile(PathBuf),
    #[error("HTTP response declared {declared} bytes, exceeding the {maximum}-byte limit")]
    HttpContentTooLarge { declared: u64, maximum: u64 },
    #[error("HTTP media exceeded the {maximum}-byte limit")]
    HttpBodyTooLarge { maximum: u64 },
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("unsupported or invalid audio format: {0}")]
    Decode(String),
    #[error("no audio media is loaded")]
    NoMediaLoaded,
    #[error("audio output is supported only on macOS desktop")]
    UnsupportedPlatform,
    #[error("failed to open the default audio output: {0}")]
    AudioOutput(String),
    #[error("seek to {position_milliseconds} ms is beyond duration {duration_milliseconds} ms")]
    SeekOutOfRange {
        position_milliseconds: u64,
        duration_milliseconds: u64,
    },
    #[error("decoder does not support seeking to {position_milliseconds} ms: {reason}")]
    SeekUnsupported {
        position_milliseconds: u64,
        reason: String,
    },
    #[error("playback rate {requested} is unsupported; expected {minimum}..={maximum}")]
    UnsupportedRate {
        requested: f32,
        minimum: f32,
        maximum: f32,
    },
    #[error(
        "clip bounds must satisfy start < end, got {start_milliseconds}..{end_milliseconds} ms"
    )]
    InvalidClipBounds {
        start_milliseconds: u64,
        end_milliseconds: u64,
    },
    #[error("clip end {end_milliseconds} ms exceeds media duration {duration_milliseconds} ms")]
    ClipEndOutOfRange {
        end_milliseconds: u64,
        duration_milliseconds: u64,
    },
    #[error("clip input and output paths must differ")]
    ClipInputEqualsOutput,
    #[error("clip output already exists: {}", .0.display())]
    OutputExists(PathBuf),
    #[error("clip export ended after {actual_frames} frames; expected {expected_frames}")]
    TruncatedClip {
        actual_frames: u64,
        expected_frames: u64,
    },
    #[error("WAV encoding failed: {0}")]
    Wav(#[from] hound::Error),
    #[error("encoded clip verification failed: {0}")]
    ClipVerification(String),
}

pub type Result<T> = std::result::Result<T, MediaError>;
