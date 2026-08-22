use std::{sync::Mutex, time::Duration};

use crate::{MediaError, PreparedMedia, Result};

fn milliseconds(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(target_os = "macos")]
pub(crate) struct ActivePlayback {
    sink: rodio::Sink,
    timeline: Mutex<MediaTimeline>,
    _stream: rodio::OutputStream,
}

#[cfg(target_os = "macos")]
impl ActivePlayback {
    pub(crate) fn start(media: &PreparedMedia, position: Duration, rate: f32) -> Result<Self> {
        let decoder = media.decoder()?;
        let stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|error| MediaError::AudioOutput(error.to_string()))?;
        let sink = rodio::Sink::connect_new(stream.mixer());
        sink.pause();
        sink.set_speed(rate);
        sink.append(decoder);
        let timeline = MediaTimeline::new(position, rate);
        if !position.is_zero() {
            seek_sink(&sink, timeline.sink_position(), position)?;
        }
        sink.play();
        Ok(Self {
            sink,
            timeline: Mutex::new(timeline),
            _stream: stream,
        })
    }

    pub(crate) fn play(&self) {
        self.sink.play();
    }

    pub(crate) fn pause(&self) {
        self.sink.pause();
    }

    pub(crate) fn stop(self) {
        self.sink.stop();
    }

    pub(crate) fn seek(&self, position: Duration) -> Result<()> {
        let mut timeline = self.timeline.lock().expect("playback timeline poisoned");
        let sink_position = sink_position_for(position, timeline.rate());
        seek_sink(&self.sink, sink_position, position)?;
        timeline.rebase(position, sink_position);
        Ok(())
    }

    pub(crate) fn set_rate(&self, rate: f32) -> Result<()> {
        let mut timeline = self.timeline.lock().expect("playback timeline poisoned");
        let media_position = timeline.position(self.sink.get_pos());
        let previous_rate = timeline.rate();
        let sink_position = sink_position_for(media_position, rate);
        self.sink.set_speed(rate);
        if let Err(error) = seek_sink(&self.sink, sink_position, media_position) {
            self.sink.set_speed(previous_rate);
            return Err(error);
        }
        timeline.set_rate(media_position, sink_position, rate);
        Ok(())
    }

    pub(crate) fn position(&self) -> Duration {
        self.timeline
            .lock()
            .expect("playback timeline poisoned")
            .position(self.sink.get_pos())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}

#[derive(Debug)]
struct MediaTimeline {
    media_anchor: Duration,
    sink_anchor: Duration,
    rate: f32,
}

impl MediaTimeline {
    fn new(position: Duration, rate: f32) -> Self {
        Self {
            media_anchor: position,
            sink_anchor: sink_position_for(position, rate),
            rate,
        }
    }

    fn position(&self, sink_position: Duration) -> Duration {
        self.media_anchor.saturating_add(scale_duration(
            sink_position.saturating_sub(self.sink_anchor),
            self.rate,
        ))
    }

    fn sink_position(&self) -> Duration {
        self.sink_anchor
    }

    fn rate(&self) -> f32 {
        self.rate
    }

    fn rebase(&mut self, media_position: Duration, sink_position: Duration) {
        self.media_anchor = media_position;
        self.sink_anchor = sink_position;
    }

    fn set_rate(&mut self, media_position: Duration, sink_position: Duration, rate: f32) {
        self.rebase(media_position, sink_position);
        self.rate = rate;
    }
}

fn sink_position_for(media_position: Duration, rate: f32) -> Duration {
    Duration::from_secs_f64(media_position.as_secs_f64() / f64::from(rate))
}

fn scale_duration(duration: Duration, factor: f32) -> Duration {
    Duration::from_secs_f64(duration.as_secs_f64() * f64::from(factor))
}

#[cfg(target_os = "macos")]
fn seek_sink(sink: &rodio::Sink, sink_position: Duration, media_position: Duration) -> Result<()> {
    sink.try_seek(sink_position)
        .map_err(|error| MediaError::SeekUnsupported {
            position_milliseconds: milliseconds(media_position),
            reason: error.to_string(),
        })
}

#[cfg(not(target_os = "macos"))]
pub(crate) struct ActivePlayback;

#[cfg(not(target_os = "macos"))]
impl ActivePlayback {
    pub(crate) fn start(_: &PreparedMedia, _: Duration, _: f32) -> Result<Self> {
        Err(MediaError::UnsupportedPlatform)
    }

    pub(crate) fn play(&self) {}
    pub(crate) fn pause(&self) {}
    pub(crate) fn stop(self) {}
    pub(crate) fn seek(&self, _: Duration) -> Result<()> {
        Err(MediaError::UnsupportedPlatform)
    }
    pub(crate) fn set_rate(&self, _: f32) -> Result<()> {
        Err(MediaError::UnsupportedPlatform)
    }
    pub(crate) fn position(&self) -> Duration {
        Duration::ZERO
    }
    pub(crate) fn is_empty(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaTimeline, sink_position_for};
    use std::time::Duration;

    #[test]
    fn timeline_reports_absolute_media_time_from_nonzero_start() {
        let timeline = MediaTimeline::new(Duration::from_secs(30), 2.0);

        assert_eq!(timeline.sink_position(), Duration::from_secs(15));
        assert_eq!(
            timeline.position(Duration::from_millis(15_250)),
            Duration::from_millis(30_500)
        );
    }

    #[test]
    fn timeline_rebases_rate_changes_without_rescaling_elapsed_media() {
        let mut timeline = MediaTimeline::new(Duration::from_secs(10), 2.0);
        let current = timeline.position(Duration::from_millis(5_500));
        assert_eq!(current, Duration::from_secs(11));

        let new_sink_position = sink_position_for(current, 0.5);
        timeline.set_rate(current, new_sink_position, 0.5);

        assert_eq!(new_sink_position, Duration::from_secs(22));
        assert_eq!(
            timeline.position(Duration::from_millis(22_500)),
            Duration::from_millis(11_250)
        );
    }

    #[test]
    fn seek_targets_compensate_for_sink_speed_scaling() {
        assert_eq!(
            sink_position_for(Duration::from_millis(1_500), 1.5),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn explicit_seek_rebases_the_absolute_timeline() {
        let mut timeline = MediaTimeline::new(Duration::from_secs(10), 2.0);
        let media_position = Duration::from_secs(45);
        let sink_position = sink_position_for(media_position, timeline.rate());
        timeline.rebase(media_position, sink_position);

        assert_eq!(timeline.position(sink_position), media_position);
        assert_eq!(
            timeline.position(sink_position + Duration::from_millis(250)),
            Duration::from_millis(45_500)
        );
    }
}
