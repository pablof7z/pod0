use super::*;

pub(super) fn arm_timer(
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
