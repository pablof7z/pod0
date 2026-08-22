use std::io::Write;

use pod0_cli::{HostConfig, HostRequestEnvelope, execute_host_request};
use pod0_domain::PlaybackSeekReason;
use pod0_facade::{
    CancellationId, CommandId, EpisodeId, HostObservation, HostRequestId, HostRequest,
    NativeTimerMode, PlaybackHostState, PlaybackRatePermille, PlaybackTransitionCue, StateRevision,
};

const SAMPLE_RATE: u32 = 8_000;

fn envelope(request: HostRequest) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(2, 1),
        command_id: CommandId::from_parts(2, 2),
        cancellation_id: CancellationId::from_parts(2, 3),
        issued_revision: StateRevision::new(1),
        deadline_at: None,
        request,
    }
}

fn write_wav(path: &std::path::Path, samples: &[i16]) {
    let data_len = u32::try_from(samples.len() * 2).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::File::create(path)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
}

fn run(config: &HostConfig, request: HostRequest) -> HostObservation {
    execute_host_request(config.clone(), envelope(request))
        .unwrap()
        .unwrap()
}

fn assert_state(observation: &HostObservation, expected: PlaybackHostState) {
    let HostObservation::PlaybackObserved { value } = observation else {
        panic!("expected PlaybackObserved, got {observation:?}");
    };
    assert_eq!(value.state, expected, "got {value:?}");
}

#[test]
fn playback_loads_observes_seeks_pauses_and_stops_real_media() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let wav = directory.path().join("tone.wav");
    // 8000 samples at 8000 Hz = 1 second of real audio.
    let samples: Vec<i16> = (0..8000).map(|i| {
        let phase = (i as f32 / 8.0).sin() * 0.3;
        (phase * 32_000.0) as i16
    }).collect();
    write_wav(&wav, &samples);
    let audio_url = format!("file://{}", wav.to_string_lossy());
    let config = HostConfig::empty();
    let episode = EpisodeId::from_parts(3, 4);

    let observation = run(&config, HostRequest::LoadMedia {
        episode_id: episode,
        audio_url: audio_url.clone(),
        start_position_milliseconds: 0,
    });
    let HostObservation::PlaybackObserved { value } = &observation else {
        panic!("load must produce PlaybackObserved, got {observation:?}");
    };
    assert_eq!(value.state, PlaybackHostState::Prepared);
    assert_eq!(value.episode_id, Some(episode));
    assert_eq!(value.position_milliseconds, 0);
    assert!(
        value.duration_milliseconds >= 900 && value.duration_milliseconds <= 1100,
        "duration must reflect the real ~1s media, got {}",
        value.duration_milliseconds
    );

    let observation = run(&config, HostRequest::ObservePlayback {
        episode_id: Some(episode),
        minimum_interval_milliseconds: 500,
    });
    assert_state(&observation, PlaybackHostState::Prepared);

    let observation = run(&config, HostRequest::Seek {
        episode_id: episode,
        position_milliseconds: 400,
        reason: PlaybackSeekReason::UserRequested,
        chapter_context: None,
    });
    let HostObservation::PlaybackObserved { value } = &observation else {
        panic!("seek must produce PlaybackObserved, got {observation:?}");
    };
    assert_eq!(value.position_milliseconds, 400);

    let observation = run(&config, HostRequest::SetRate {
        episode_id: episode,
        rate: PlaybackRatePermille { value: 1500 },
    });
    assert_state(&observation, PlaybackHostState::Prepared);

    let observation = run(&config, HostRequest::Pause { episode_id: episode });
    assert_state(&observation, PlaybackHostState::Paused);

    let observation = run(&config, HostRequest::StopPlayback { episode_id: episode });
    assert_state(&observation, PlaybackHostState::Idle);

    // Operating on the wrong episode fails honestly (no media loaded for it).
    let other = EpisodeId::from_parts(9, 9);
    let observation = run(&config, HostRequest::Pause { episode_id: other });
    assert_state(&observation, PlaybackHostState::Failed);
}

#[test]
fn playback_arm_end_of_episode_timer_observes_without_a_fake_pause() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let wav = directory.path().join("short.wav");
    write_wav(&wav, &vec![0_i16; 800]);
    let audio_url = format!("file://{}", wav.to_string_lossy());
    let config = HostConfig::empty();
    let episode = EpisodeId::from_parts(5, 6);

    let _ = run(&config, HostRequest::LoadMedia {
        episode_id: episode,
        audio_url,
        start_position_milliseconds: 0,
    });
    let observation = run(&config, HostRequest::ArmNativeTimer {
        episode_id: episode,
        mode: NativeTimerMode::EndOfEpisode,
    });
    assert_state(&observation, PlaybackHostState::Prepared);

    let observation = run(&config, HostRequest::CancelNativeTimer { episode_id: episode });
    assert_state(&observation, PlaybackHostState::Prepared);
}

#[test]
fn playback_play_starts_real_audio_output_or_fails_honestly_without_audio_device() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let wav = directory.path().join("play.wav");
    write_wav(&wav, &(0..800).map(|i| ((i as f32 / 8.0).sin() * 30_000.0) as i16).collect::<Vec<_>>());
    let audio_url = format!("file://{}", wav.to_string_lossy());
    let config = HostConfig::empty();
    let episode = EpisodeId::from_parts(7, 8);

    let _ = run(&config, HostRequest::LoadMedia {
        episode_id: episode,
        audio_url,
        start_position_milliseconds: 0,
    });
    let observation = run(&config, HostRequest::Play {
        episode_id: episode,
        transition_cue: PlaybackTransitionCue::Immediate,
    });
    let HostObservation::PlaybackObserved { value } = &observation else {
        panic!("play must produce PlaybackObserved, got {observation:?}");
    };
    // Real playback either starts (device present) or fails honestly with a
    // real audio-output error. It must never report a fabricated success.
    assert!(
        matches!(value.state, PlaybackHostState::Playing | PlaybackHostState::Failed),
        "play must really attempt audio output, got {value:?}"
    );
    assert_eq!(value.episode_id, Some(episode));
}