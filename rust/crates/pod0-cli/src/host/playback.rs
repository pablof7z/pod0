use std::sync::Arc;
use std::time::Duration;

use pod0_facade::{
    EpisodeId, HostObservation, HostRequest, NativeTimerMode, PlaybackAudioRoute,
    PlaybackHostState, PlaybackInterruption, PlaybackLifecycleObservation, PlaybackRatePermille,
    PlaybackTransitionCue,
};
use pod0_portable_media::{
    CancellationToken, MediaError, MediaLoader, MediaPlayer, PlaybackState, MAX_PLAYBACK_RATE,
    MIN_PLAYBACK_RATE,
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

pub(crate) fn execute(
    request: &HostRequest,
    runtime: &tokio::runtime::Handle,
) -> HostObservation {
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
                    }
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

fn arm_timer(
    player: &mut HostPlayer,
    episode_id: EpisodeId,
    mode: NativeTimerMode,
) -> HostObservation {
    player.cancel_timer();
    match mode {
        NativeTimerMode::Duration {
            duration_milliseconds,
        } => {
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let timer_cancel = Arc::clone(&cancel);
            let timer_fired = Arc::clone(&fired);
            // Real detached timer. It only touches Send Arc<AtomicBool> flags,
            // never the !Send media player. When it elapses it sets `fired`;
            // the player's own thread applies the pause at the next observation
            // (drain_fired_timer). CancelNativeTimer sets `cancel` so the
            // thread exits without firing.
            std::thread::Builder::new()
                .name("pod0-sleep-timer".to_owned())
                .spawn(move || {
                    let total = Duration::from_millis(duration_milliseconds);
                    let mut elapsed = Duration::ZERO;
                    while elapsed < total {
                        if timer_cancel.load(std::sync::atomic::Ordering::SeqCst) {
                            return;
                        }
                        let step = Duration::from_millis(100).min(total - elapsed);
                        std::thread::sleep(step);
                        elapsed += step;
                    }
                    if !timer_cancel.load(std::sync::atomic::Ordering::SeqCst) {
                        timer_fired.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                })
                .ok();
            player.timer_cancel = Some(cancel);
            player.timer_fired = Some(fired);
            observe(player, Some(episode_id))
        }
        NativeTimerMode::EndOfEpisode => {
            // End-of-episode is intrinsic to the media player: it reports
            // `ended` and transitions to the Ended state on its own. No
            // external timer is required.
            observe(player, Some(episode_id))
        }
        NativeTimerMode::Unsupported { wire_code } => HostObservation::PlaybackObserved {
            value: PlaybackLifecycleObservation {
                episode_id: Some(episode_id),
                state: PlaybackHostState::Unsupported { wire_code },
                position_milliseconds: 0,
                duration_milliseconds: 0,
                route: PlaybackAudioRoute::Unsupported { wire_code },
                interruption: PlaybackInterruption::Unsupported { wire_code },
                ended: false,
            },
        },
    }
}

fn map_state(state: PlaybackState) -> PlaybackHostState {
    match state {
        PlaybackState::Idle => PlaybackHostState::Idle,
        PlaybackState::Loading => PlaybackHostState::Loading,
        PlaybackState::Prepared => PlaybackHostState::Prepared,
        PlaybackState::Playing => PlaybackHostState::Playing,
        PlaybackState::Paused => PlaybackHostState::Paused,
        PlaybackState::Ended => PlaybackHostState::Idle,
        PlaybackState::Failed => PlaybackHostState::Failed,
    }
}

fn idle_observation(episode_id: Option<EpisodeId>) -> PlaybackLifecycleObservation {
    PlaybackLifecycleObservation {
        episode_id,
        state: PlaybackHostState::Idle,
        position_milliseconds: 0,
        duration_milliseconds: 0,
        route: PlaybackAudioRoute::BuiltIn,
        interruption: PlaybackInterruption::None,
        ended: false,
    }
}

fn failed_state(episode_id: EpisodeId) -> HostObservation {
    HostObservation::PlaybackObserved {
        value: failed_observation_value(Some(episode_id)),
    }
}

fn failed_observation(episode_id: Option<EpisodeId>, _error: &MediaError) -> HostObservation {
    // The durable playback contract carries no free-text detail on
    // PlaybackObserved; the Failed state signals the failure. The error text is
    // intentionally not fabricated into another observation kind (no mocks).
    HostObservation::PlaybackObserved {
        value: failed_observation_value(episode_id),
    }
}

fn failed_observation_value(episode_id: Option<EpisodeId>) -> PlaybackLifecycleObservation {
    PlaybackLifecycleObservation {
        episode_id,
        state: PlaybackHostState::Failed,
        position_milliseconds: 0,
        duration_milliseconds: 0,
        route: PlaybackAudioRoute::BuiltIn,
        interruption: PlaybackInterruption::None,
        ended: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{HostPlayer, episode_id_of};
    use pod0_domain::PlaybackSeekReason;
    use pod0_facade::{EpisodeId, HostRequest, PlaybackRatePermille, PlaybackTransitionCue};

    #[test]
    fn episode_id_is_resolved_for_every_playback_request_kind() {
        let id = EpisodeId::from_parts(1, 2);
        for request in [
            HostRequest::LoadMedia {
                episode_id: id,
                audio_url: "file:///tmp/x.wav".to_owned(),
                start_position_milliseconds: 0,
            },
            HostRequest::Play {
                episode_id: id,
                transition_cue: PlaybackTransitionCue::Immediate,
            },
            HostRequest::Pause { episode_id: id },
            HostRequest::Seek {
                episode_id: id,
                position_milliseconds: 1000,
                reason: PlaybackSeekReason::UserRequested,
                chapter_context: None,
            },
            HostRequest::SetRate {
                episode_id: id,
                rate: PlaybackRatePermille { value: 1500 },
            },
            HostRequest::StopPlayback { episode_id: id },
            HostRequest::ArmNativeTimer {
                episode_id: id,
                mode: pod0_facade::NativeTimerMode::EndOfEpisode,
            },
            HostRequest::CancelNativeTimer { episode_id: id },
        ] {
            assert_eq!(episode_id_of(&request), Some(id));
        }
        assert_eq!(
            episode_id_of(&HostRequest::ObservePlayback {
                episode_id: Some(id),
                minimum_interval_milliseconds: 500,
            }),
            Some(id)
        );
        assert_eq!(
            episode_id_of(&HostRequest::ObservePlayback {
                episode_id: None,
                minimum_interval_milliseconds: 500,
            }),
            None
        );
    }

    #[test]
    fn player_can_be_constructed() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(HostPlayer::new(runtime.handle().clone()).is_ok());
    }
}