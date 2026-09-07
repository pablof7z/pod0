use std::sync::Arc;
use std::time::Duration;

use pod0_facade::{
    EpisodeId, HostObservation, HostRequest, NativeTimerMode, PlaybackAudioRoute,
    PlaybackHostState, PlaybackInterruption, PlaybackLifecycleObservation, PlaybackRatePermille,
    PlaybackTransitionCue,
};
mod observation;
mod timer;

use observation::*;
use timer::arm_timer;

use pod0_portable_media::{
    CancellationToken, MAX_PLAYBACK_RATE, MIN_PLAYBACK_RATE, MediaError, MediaLoader, MediaPlayer,
    PlaybackState,
};

/// Real, host-owned playback state. `MediaPlayer` is `!Send` on macOS (rodio's
/// audio output stream), so the player lives in a `thread_local` and is created
/// on whichever thread calls `execute` — the host pump worker, or a test/agent
/// thread driving `execute_host_request`. The sleep timer runs on a detached
/// thread that only touches `Arc<AtomicBool>` flags (Send); the actual pause is
/// applied on the player's own thread at the next observation.
struct HostPlayer {
    player: MediaPlayer,
    loaded_episode: Option<EpisodeId>,
    timer_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    timer_fired: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl HostPlayer {
    fn new(runtime: tokio::runtime::Handle) -> Result<Self, MediaError> {
        let loader =
            MediaLoader::new_with_handle(pod0_portable_media::HttpLoadOptions::default(), runtime)?;
        Ok(Self {
            player: MediaPlayer::new(loader),
            loaded_episode: None,
            timer_cancel: None,
            timer_fired: None,
        })
    }

    fn cancel_timer(&mut self) {
        if let Some(cancel) = self.timer_cancel.take() {
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        self.timer_fired = None;
    }

    /// Apply a fired sleep timer by pausing the real player. Called on the
    /// player's own thread before producing an observation.
    fn drain_fired_timer(&mut self) {
        if let Some(fired) = self.timer_fired.as_ref()
            && fired.load(std::sync::atomic::Ordering::SeqCst)
        {
            let _ = self.player.pause();
            self.timer_cancel = None;
            self.timer_fired = None;
        }
    }
}

thread_local! {
    static PLAYER: std::cell::RefCell<Option<HostPlayer>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn execute(request: &HostRequest, runtime: &tokio::runtime::Handle) -> HostObservation {
    let Some(episode_id) = episode_id_of(request) else {
        return HostObservation::PlaybackObserved {
            value: idle_observation(None),
        };
    };
    PLAYER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            match HostPlayer::new(runtime.clone()) {
                Ok(player) => *slot = Some(player),
                Err(_) => {
                    return HostObservation::PlaybackObserved {
                        value: failed_observation_value(Some(episode_id)),
                    };
                }
            }
        }
        let player = slot.as_mut().expect("player was just initialized");
        dispatch(player, request, episode_id)
    })
}

fn dispatch(
    player: &mut HostPlayer,
    request: &HostRequest,
    episode_id: EpisodeId,
) -> HostObservation {
    player.drain_fired_timer();
    match request {
        HostRequest::LoadMedia {
            audio_url,
            start_position_milliseconds,
            ..
        } => load(player, episode_id, audio_url, *start_position_milliseconds),
        HostRequest::Play { transition_cue, .. } => play(player, episode_id, *transition_cue),
        HostRequest::Pause { .. } => pause(player, episode_id),
        HostRequest::Seek {
            position_milliseconds,
            ..
        } => seek(player, episode_id, *position_milliseconds),
        HostRequest::SetRate { rate, .. } => set_rate(player, episode_id, *rate),
        HostRequest::ObservePlayback { .. } => observe(player, Some(episode_id)),
        HostRequest::StopPlayback { .. } => stop(player, episode_id),
        HostRequest::ArmNativeTimer { mode, .. } => arm_timer(player, episode_id, *mode),
        HostRequest::CancelNativeTimer { .. } => {
            player.cancel_timer();
            observe(player, Some(episode_id))
        }
        _ => HostObservation::PlaybackObserved {
            value: idle_observation(Some(episode_id)),
        },
    }
}

fn episode_id_of(request: &HostRequest) -> Option<EpisodeId> {
    match request {
        HostRequest::LoadMedia { episode_id, .. }
        | HostRequest::Play { episode_id, .. }
        | HostRequest::Pause { episode_id }
        | HostRequest::Seek { episode_id, .. }
        | HostRequest::SetRate { episode_id, .. }
        | HostRequest::ArmNativeTimer { episode_id, .. }
        | HostRequest::CancelNativeTimer { episode_id }
        | HostRequest::StopPlayback { episode_id } => Some(*episode_id),
        HostRequest::ObservePlayback { episode_id, .. } => *episode_id,
        _ => None,
    }
}

fn load(
    player: &mut HostPlayer,
    episode_id: EpisodeId,
    audio_url: &str,
    start_position_milliseconds: u64,
) -> HostObservation {
    player.cancel_timer();
    let source = match pod0_portable_media::MediaSource::from_location(audio_url) {
        Ok(source) => source,
        Err(error) => return failed_observation(Some(episode_id), &error),
    };
    let cancellation = CancellationToken::new();
    let start = Duration::from_millis(start_position_milliseconds);
    match player.player.load(source, start, &cancellation) {
        Ok(metadata) => {
            player.loaded_episode = Some(episode_id);
            HostObservation::PlaybackObserved {
                value: PlaybackLifecycleObservation {
                    episode_id: Some(episode_id),
                    state: PlaybackHostState::Prepared,
                    position_milliseconds: start_position_milliseconds,
                    duration_milliseconds: metadata
                        .duration
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or_default(),
                    route: PlaybackAudioRoute::BuiltIn,
                    interruption: PlaybackInterruption::None,
                    ended: false,
                },
            }
        }
        Err(error) => {
            player.loaded_episode = None;
            failed_observation(Some(episode_id), &error)
        }
    }
}

fn play(
    player: &mut HostPlayer,
    episode_id: EpisodeId,
    _cue: PlaybackTransitionCue,
) -> HostObservation {
    if player.loaded_episode != Some(episode_id) {
        return failed_state(episode_id);
    }
    match player.player.play() {
        Ok(()) => observe(player, Some(episode_id)),
        Err(error) => failed_observation(Some(episode_id), &error),
    }
}

fn pause(player: &mut HostPlayer, episode_id: EpisodeId) -> HostObservation {
    if player.loaded_episode != Some(episode_id) {
        return failed_state(episode_id);
    }
    match player.player.pause() {
        Ok(()) => observe(player, Some(episode_id)),
        Err(error) => failed_observation(Some(episode_id), &error),
    }
}

fn seek(
    player: &mut HostPlayer,
    episode_id: EpisodeId,
    position_milliseconds: u64,
) -> HostObservation {
    if player.loaded_episode != Some(episode_id) {
        return failed_state(episode_id);
    }
    match player
        .player
        .seek(Duration::from_millis(position_milliseconds))
    {
        Ok(()) => observe(player, Some(episode_id)),
        Err(error) => failed_observation(Some(episode_id), &error),
    }
}

fn set_rate(
    player: &mut HostPlayer,
    episode_id: EpisodeId,
    rate: PlaybackRatePermille,
) -> HostObservation {
    if player.loaded_episode != Some(episode_id) {
        return failed_state(episode_id);
    }
    let rate_f32 = f32::from(rate.value) / 1000.0;
    if !(MIN_PLAYBACK_RATE..=MAX_PLAYBACK_RATE).contains(&rate_f32) {
        return failed_state(episode_id);
    }
    match player.player.set_rate(rate_f32) {
        Ok(()) => observe(player, Some(episode_id)),
        Err(error) => failed_observation(Some(episode_id), &error),
    }
}

fn stop(player: &mut HostPlayer, episode_id: EpisodeId) -> HostObservation {
    player.cancel_timer();
    player.player.stop();
    player.loaded_episode = None;
    HostObservation::PlaybackObserved {
        value: idle_observation(Some(episode_id)),
    }
}

fn observe(player: &mut HostPlayer, episode_id: Option<EpisodeId>) -> HostObservation {
    let observation = player.player.observation();
    let episode_id = episode_id.or(player.loaded_episode);
    HostObservation::PlaybackObserved {
        value: PlaybackLifecycleObservation {
            episode_id,
            state: map_state(observation.state),
            position_milliseconds: observation.position_milliseconds(),
            duration_milliseconds: observation.duration_milliseconds(),
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: observation.ended,
        },
    }
}

#[cfg(test)]
mod tests;
