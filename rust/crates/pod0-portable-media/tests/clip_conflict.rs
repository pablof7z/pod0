#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use pod0_portable_media::{CancellationToken, ClipExportRequest, MediaError, export_clip};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const PARTIAL_COLLISIONS: u64 = 16;

#[test]
fn export_preserves_conflicting_partial_files() {
    let directory = TestDirectory::new();
    let input = directory.path().join("input.wav");
    let output = directory.path().join("output.wav");
    write_wav(&input);
    let collisions = partial_paths(&output);
    for (index, path) in collisions.iter().enumerate() {
        fs::write(path, format!("existing partial {index}")).expect("create partial collision");
    }

    export_clip(
        &ClipExportRequest {
            input,
            output: output.clone(),
            start_milliseconds: 0,
            end_milliseconds: 250,
            overwrite: false,
        },
        &CancellationToken::new(),
    )
    .expect("export around partial file collisions");

    assert!(output.is_file());
    for (index, path) in collisions.iter().enumerate() {
        assert_eq!(
            fs::read_to_string(path).expect("read partial collision"),
            format!("existing partial {index}")
        );
    }
}

#[test]
fn export_preserves_conflicting_partial_symlinks_and_their_target() {
    let directory = TestDirectory::new();
    let input = directory.path().join("input.wav");
    let output = directory.path().join("output.wav");
    let target = directory.path().join("symlink-target");
    write_wav(&input);
    fs::write(&target, b"do not clobber").expect("create symlink target");
    let collisions = partial_paths(&output);
    for path in &collisions {
        symlink(&target, path).expect("create partial symlink collision");
    }

    export_clip(
        &ClipExportRequest {
            input,
            output: output.clone(),
            start_milliseconds: 0,
            end_milliseconds: 250,
            overwrite: false,
        },
        &CancellationToken::new(),
    )
    .expect("export around partial symlink collisions");

    assert!(output.is_file());
    assert_eq!(
        fs::read(&target).expect("read symlink target"),
        b"do not clobber"
    );
    for path in collisions {
        assert!(
            fs::symlink_metadata(path)
                .expect("inspect partial symlink collision")
                .file_type()
                .is_symlink()
        );
    }
}

#[test]
fn export_without_overwrite_preserves_a_conflicting_destination_entry() {
    let directory = TestDirectory::new();
    let input = directory.path().join("input.wav");
    let output = directory.path().join("output.wav");
    write_wav(&input);
    symlink("missing-target", &output).expect("create conflicting destination");

    let error = export_clip(
        &ClipExportRequest {
            input,
            output: output.clone(),
            start_milliseconds: 0,
            end_milliseconds: 250,
            overwrite: false,
        },
        &CancellationToken::new(),
    )
    .expect_err("reject destination created before publication");

    assert!(matches!(error, MediaError::OutputExists(path) if path == output));
    assert!(
        fs::symlink_metadata(&output)
            .expect("inspect conflicting destination")
            .file_type()
            .is_symlink()
    );
}

fn write_wav(path: &Path) {
    let specification = hound::WavSpec {
        channels: 1,
        sample_rate: 8_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, specification).expect("create WAV");
    for sample in 0..4_000 {
        writer.write_sample(sample as i16).expect("write sample");
    }
    writer.finalize().expect("finalize WAV");
}

fn partial_paths(output: &Path) -> Vec<PathBuf> {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .expect("UTF-8 output name");
    (0..PARTIAL_COLLISIONS)
        .map(|sequence| {
            output.with_file_name(format!(".{name}.partial-{}-{sequence}", std::process::id()))
        })
        .collect()
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("clip-conflict-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("create test directory");
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
