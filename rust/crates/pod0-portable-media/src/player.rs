use std::time::Duration;

use crate::{
    CancellationToken, MediaError, MediaLoader, MediaMetadata, MediaSource, PreparedMedia, Result,
    playback_backend::ActivePlayback,
};

pub const MIN_PLAYBACK_RATE: f32 = 0.5;
pub const MAX_PLAYBACK_RATE: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Loading,
    Prepared,
    Playing,
    Paused,
    Ended,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackObservation {
    pub state: PlaybackState,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub ended: bool,
    pub failure: Option<String>,
}

impl PlaybackObservation {
    #[must_use]
    pub fn position_milliseconds(&self) -> u64 {
        milliseconds(self.position)
    }

    /// Returns zero when the decoder cannot determine a duration.
    #[must_use]
    pub fn duration_milliseconds(&self) -> u64 {
        self.duration.map(milliseconds).unwrap_or_default()
    }
}

pub struct MediaPlayer {
    loader: MediaLoader,
    prepared: Option<PreparedMedia>,
    active: Option<ActivePlayback>,
    state: PlaybackState,
    position: Duration,
    rate: f32,
    failure: Option<String>,
}

impl std::fmt::Debug for MediaPlayer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MediaPlayer")
            .field("state", &self.state)
            .field("position", &self.position)
            .field("rate", &self.rate)
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl MediaPlayer {
    #[must_use]
    pub fn new(loader: MediaLoader) -> Self {
        Self {
            loader,
            prepared: None,
            active: None,
            state: PlaybackState::Idle,
            position: Duration::ZERO,
            rate: 1.0,
            failure: None,
        }
    }

    pub fn load(
        &mut self,
        source: impl Into<MediaSource>,
        start_position: Duration,
        cancellation: &CancellationToken,
    ) -> Result<MediaMetadata> {
        self.shutdown_active();
        self.prepared = None;
        self.position = Duration::ZERO;
        self.state = PlaybackState::Loading;
        self.failure = None;
        let prepared = match self.loader.load(source, cancellation) {
            Ok(prepared) => prepared,
            Err(error) => return self.fail(error),
        };
        if let Some(duration) = prepared.metadata().duration
            && start_position > duration
        {
            return self.fail(MediaError::SeekOutOfRange {
                position_milliseconds: milliseconds(start_position),
                duration_milliseconds: milliseconds(duration),
            });
        }
        self.position = start_position;
        self.state = PlaybackState::Prepared;
        let metadata = prepared.metadata().clone();
        self.prepared = Some(prepared);
        Ok(metadata)
    }

    pub fn play(&mut self) -> Result<()> {
        if self.active.is_none() {
            let prepared = self.prepared.as_ref().ok_or(MediaError::NoMediaLoaded)?;
            let active = match ActivePlayback::start(prepared, self.position, self.rate) {
                Ok(active) => active,
                Err(error) => return self.fail(error),
            };
            self.active = Some(active);
        } else if let Some(active) = &self.active {
            active.play();
        }
        self.state = PlaybackState::Playing;
        self.failure = None;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        self.prepared.as_ref().ok_or(MediaError::NoMediaLoaded)?;
        if let Some(active) = &self.active {
            active.pause();
            self.position = active.position();
        }
        self.state = PlaybackState::Paused;
        Ok(())
    }

    pub fn seek(&mut self, position: Duration) -> Result<()> {
        let duration = self
            .prepared
            .as_ref()
            .ok_or(MediaError::NoMediaLoaded)?
            .metadata()
            .duration;
        if let Some(duration) = duration
            && position > duration
        {
            return Err(MediaError::SeekOutOfRange {
                position_milliseconds: milliseconds(position),
                duration_milliseconds: milliseconds(duration),
            });
        }
        if let Some(active) = &self.active {
            active.seek(position)?;
        }
        self.position = position;
        if self.state == PlaybackState::Ended {
            self.state = PlaybackState::Paused;
        }
        Ok(())
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        if !rate.is_finite() || !(MIN_PLAYBACK_RATE..=MAX_PLAYBACK_RATE).contains(&rate) {
            return Err(MediaError::UnsupportedRate {
                requested: rate,
                minimum: MIN_PLAYBACK_RATE,
                maximum: MAX_PLAYBACK_RATE,
            });
        }
        if let Some(active) = &self.active {
            active.set_rate(rate)?;
        }
        self.rate = rate;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.shutdown_active();
        self.prepared = None;
        self.position = Duration::ZERO;
        self.state = PlaybackState::Idle;
        self.failure = None;
    }

    pub fn cancel(&mut self) {
        self.stop();
    }

    pub fn observation(&mut self) -> PlaybackObservation {
        let mut reached_end = false;
        if let Some(active) = &self.active {
            self.position = active.position();
            if self.state == PlaybackState::Playing && active.is_empty() {
                self.state = PlaybackState::Ended;
                reached_end = true;
            }
        }
        if reached_end {
            self.shutdown_active();
            if let Some(duration) = self
                .prepared
                .as_ref()
                .and_then(|prepared| prepared.metadata().duration)
            {
                self.position = duration;
            }
        }
        PlaybackObservation {
            state: self.state,
            position: self.position,
            duration: self
                .prepared
                .as_ref()
                .and_then(|prepared| prepared.metadata().duration),
            ended: self.state == PlaybackState::Ended,
            failure: self.failure.clone(),
        }
    }

    fn shutdown_active(&mut self) {
        if let Some(active) = self.active.take() {
            active.stop();
        }
    }

    fn fail<T>(&mut self, error: MediaError) -> Result<T> {
        self.state = PlaybackState::Failed;
        self.failure = Some(error.to_string());
        Err(error)
    }
}

impl Drop for MediaPlayer {
    fn drop(&mut self) {
        self.shutdown_active();
    }
}

fn milliseconds(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
