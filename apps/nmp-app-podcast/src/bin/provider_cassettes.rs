use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use nmp_app_podcast::llm::provider_cassette::{validate_dir, CassetteStore};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "verify".to_owned());
    let dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_cassette_dir);

    match command.as_str() {
        "verify" => verify(dir),
        _ => {
            eprintln!("usage: provider-cassettes verify [cassette-dir]");
            ExitCode::FAILURE
        }
    }
}

fn verify(dir: PathBuf) -> ExitCode {
    let violations = validate_dir(&dir);
    if !violations.is_empty() {
        for violation in violations {
            eprintln!("{}: {}", violation.path.display(), violation.message);
        }
        return ExitCode::FAILURE;
    }

    match CassetteStore::load_dir(&dir) {
        Ok(store) if !store.is_empty() => {
            println!(
                "provider cassette verification passed: {} cassette(s)",
                store.len()
            );
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!("{}: no cassettes found", dir.display());
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("{}: {error}", dir.display());
            ExitCode::FAILURE
        }
    }
}

fn default_cassette_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/provider_cassettes")
}
