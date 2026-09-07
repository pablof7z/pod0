use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use pod0_portable_media::{
    CancellationToken, ClipEncoding, ClipExportRequest, HttpLoadOptions, MediaError, MediaLoader,
    MediaPlayer, MediaSource, PlaybackState, export_clip,
};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const SAMPLE_RATE: u32 = 8_000;

#[test]
fn decodes_real_local_wav_and_exposes_timeline() {
    let directory = TestDirectory::new("decode");
    let input = directory.path().join("input.wav");
    write_sine_wave(&input, 2_000);

    let loader = loader();
    let cancellation = CancellationToken::new();
    let media = loader
        .load(MediaSource::from(input.as_path()), &cancellation)
        .expect("decode generated WAV");
    assert_eq!(media.metadata().sample_rate, SAMPLE_RATE);
    assert_eq!(media.metadata().channels, 1);
    assert_eq!(media.metadata().duration, Some(Duration::from_secs(2)));

    let mut player = MediaPlayer::new(loader);
    player
        .load(
            MediaSource::from(input.as_path()),
            Duration::from_millis(250),
            &cancellation,
        )
        .expect("prepare generated WAV");
    assert_eq!(player.observation().state, PlaybackState::Prepared);
    assert_eq!(player.observation().position_milliseconds(), 250);

    player.seek(Duration::from_millis(900)).expect("seek");
    player.set_rate(1.75).expect("supported rate");
    player.pause().expect("pause prepared media");
    let observation = player.observation();
    assert_eq!(observation.state, PlaybackState::Paused);
    assert_eq!(observation.position_milliseconds(), 900);
    assert_eq!(observation.duration_milliseconds(), 2_000);
    assert!(!observation.ended);

    let error = player.set_rate(3.01).expect_err("reject unsupported rate");
    assert!(matches!(error, MediaError::UnsupportedRate { .. }));
    player.stop();
    assert_eq!(player.observation().state, PlaybackState::Idle);
}

#[test]
fn downloads_and_decodes_wav_over_real_http() {
    let directory = TestDirectory::new("http");
    let input = directory.path().join("served.wav");
    write_sine_wave(&input, 500);
    let bytes = fs::read(&input).expect("read generated WAV");
    let (url, server) = serve_once(bytes);

    let media = loader()
        .load(MediaSource::Http(url), &CancellationToken::new())
        .expect("download and decode HTTP WAV");
    server.join().expect("HTTP fixture server");

    assert_eq!(media.metadata().sample_rate, SAMPLE_RATE);
    assert_eq!(media.metadata().channels, 1);
    assert_eq!(media.metadata().duration, Some(Duration::from_millis(500)));
}

#[test]
fn exports_and_verifies_real_pcm_wav_clip() {
    let directory = TestDirectory::new("clip");
    let input = directory.path().join("input.wav");
    let output = directory.path().join("nested/output.wav");
    write_sine_wave_with_channels(&input, 2_000, 2);

    let exported = export_clip(
        &ClipExportRequest {
            input,
            output: output.clone(),
            start_milliseconds: 500,
            end_milliseconds: 1_250,
            overwrite: false,
        },
        &CancellationToken::new(),
    )
    .expect("export clip");

    assert_eq!(exported.encoding, ClipEncoding::WavPcmSigned16LittleEndian);
    assert_eq!(exported.frames, 6_000);
    assert_eq!(exported.duration_milliseconds, 750);
    assert_eq!(exported.channels, 2);
    assert_eq!(exported.file_size_bytes, 24_044);
    assert_eq!(
        exported.file_size_bytes,
        fs::metadata(&output).unwrap().len()
    );

    let reader = hound::WavReader::open(output).expect("open exported WAV");
    assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().bits_per_sample, 16);
    assert_eq!(reader.duration(), 6_000);
}

#[test]
fn cancellation_prevents_clip_output() {
    let directory = TestDirectory::new("cancel");
    let input = directory.path().join("input.wav");
    let output = directory.path().join("output.wav");
    write_sine_wave(&input, 500);
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let error = export_clip(
        &ClipExportRequest {
            input,
            output: output.clone(),
            start_milliseconds: 0,
            end_milliseconds: 250,
            overwrite: false,
        },
        &cancellation,
    )
    .expect_err("cancel export");

    assert!(matches!(error, MediaError::Cancelled));
    assert!(!output.exists());
}

#[test]
fn unsupported_audio_fails_explicitly() {
    let directory = TestDirectory::new("unsupported");
    let input = directory.path().join("not-audio.bin");
    fs::write(&input, b"this is not encoded audio").expect("write invalid media");

    let error = loader()
        .load(
            MediaSource::from(input.as_path()),
            &CancellationToken::new(),
        )
        .expect_err("reject invalid audio");
    assert!(matches!(error, MediaError::Decode(_)));
}

#[test]
#[ignore = "manual smoke: requires an audible macOS default output device"]
fn audible_play_pause_seek_rate_and_stop_smoke() {
    let directory = TestDirectory::new("audible");
    let input = directory.path().join("tone.wav");
    write_sine_wave(&input, 2_000);
    let mut player = MediaPlayer::new(loader());
    player
        .load(
            MediaSource::from(input.as_path()),
            Duration::ZERO,
            &CancellationToken::new(),
        )
        .expect("prepare");
    player.play().expect("play through default output");
    thread::sleep(Duration::from_millis(100));
    assert!(player.observation().position > Duration::ZERO);
    player.pause().expect("pause");
    player.seek(Duration::from_millis(500)).expect("seek");
    player.set_rate(1.5).expect("set rate");
    player.play().expect("resume");
    thread::sleep(Duration::from_millis(100));
    player.stop();
    assert_eq!(player.observation().state, PlaybackState::Idle);
}

fn loader() -> MediaLoader {
    MediaLoader::new(HttpLoadOptions::default()).expect("build HTTP client")
}

fn write_sine_wave(path: &Path, duration_milliseconds: u64) {
    write_sine_wave_with_channels(path, duration_milliseconds, 1);
}

fn write_sine_wave_with_channels(path: &Path, duration_milliseconds: u64, channels: u16) {
    let specification = hound::WavSpec {
        channels,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, specification).expect("create WAV");
    let frames = duration_milliseconds * u64::from(SAMPLE_RATE) / 1_000;
    for frame in 0..frames {
        let phase = frame as f32 * 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32;
        for _ in 0..channels {
            writer
                .write_sample((phase.sin() * 8_000.0) as i16)
                .expect("write WAV sample");
        }
    }
    writer.finalize().expect("finalize WAV");
}

fn serve_once(body: Vec<u8>) -> (url::Url, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP fixture");
    let address = listener.local_addr().expect("HTTP fixture address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept HTTP request");
        let mut request = [0_u8; 2_048];
        let _ = stream.read(&mut request).expect("read HTTP request");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n",
            body.len()
        )
        .expect("write HTTP response headers");
        stream.write_all(&body).expect("write HTTP response body");
    });
    (
        url::Url::parse(&format!("http://{address}/tone.wav")).expect("fixture URL"),
        handle,
    )
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("{label}-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("create project-local test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
