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
