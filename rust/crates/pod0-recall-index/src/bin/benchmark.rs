#![allow(unsafe_code)]

use std::env;
use std::path::Path;
use std::time::{Duration, Instant};

use pod0_application::{RecallEmbeddingVector, RecallScope};
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};
use pod0_recall_index::{
    RecallIndex, RecallIndexCandidate, RecallIndexError, RecallIndexPlan, RecallIndexSpan,
    RecallSpanEmbedding,
};
use serde::Serialize;
use tempfile::tempdir;

const SPANS_PER_EPISODE: usize = 100;
const VECTOR_CANDIDATES: u16 = 20;
const LEXICAL_CANDIDATES: u16 = 20;
const TOTAL_CANDIDATES: u16 = 40;

#[derive(Serialize)]
struct BenchmarkResult {
    backend: &'static str,
    sqlite_vec_version: String,
    spans: usize,
    dimensions: usize,
    samples: usize,
    rebuild_milliseconds: f64,
    cold_query_milliseconds: f64,
    warm_query_p50_milliseconds: f64,
    warm_query_p95_milliseconds: f64,
    cancellation_response_microseconds: f64,
    candidate_count: usize,
    maximum_candidate_count: u16,
    index_bytes: u64,
    executable_bytes: u64,
    peak_resident_bytes: u64,
    private_text_leaves_process: bool,
}

struct Options {
    spans: usize,
    dimensions: usize,
    samples: usize,
}

fn main() {
    match run(parse_options()) {
        Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
        Err(error) => {
            eprintln!("recall benchmark failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run(options: Options) -> Result<BenchmarkResult, RecallIndexError> {
    let directory = tempdir()?;
    let path = directory.path().join("recall-index.sqlite");
    let rebuild_started = Instant::now();
    let mut index = RecallIndex::open(&path, options.dimensions)?;
    let sqlite_vec_version = index.sqlite_vec_version()?;
    let mut remaining = options.spans;
    let mut episode_offset = 0_u64;
    while remaining > 0 {
        let count = remaining.min(SPANS_PER_EPISODE);
        index_episode(
            &mut index,
            &generated_episode(episode_offset + 1, count),
            options.dimensions,
        )?;
        remaining -= count;
        episode_offset += 1;
    }
    index.optimize()?;
    let rebuild_duration = rebuild_started.elapsed();
    if index.stored_span_count()? != options.spans as u64 {
        return Err(RecallIndexError::InvalidInput(
            "benchmark index count does not match requested fixture",
        ));
    }
    drop(index);
    let index_bytes = directory_size(directory.path())?;
    let index = RecallIndex::open(&path, options.dimensions)?;
    let query = query_embedding(options.dimensions);
    let cold_started = Instant::now();
    let cold_candidates = retrieve(&index, &query)?;
    let cold_duration = cold_started.elapsed();
    let mut warm_samples = Vec::with_capacity(options.samples);
    let mut candidate_count = cold_candidates.len();
    for _ in 0..options.samples {
        let started = Instant::now();
        candidate_count = retrieve(&index, &query)?.len();
        warm_samples.push(started.elapsed());
    }
    warm_samples.sort_unstable();
    let cancellation = index.cancellation();
    cancellation.cancel();
    let cancellation_started = Instant::now();
    let cancelled = index.retrieve(
        &query,
        "needle evidence",
        RecallScope::Library,
        VECTOR_CANDIDATES,
        LEXICAL_CANDIDATES,
        TOTAL_CANDIDATES,
        &cancellation,
    );
    if !matches!(cancelled, Err(RecallIndexError::Cancelled)) {
        return Err(RecallIndexError::InvalidInput(
            "benchmark cancellation did not surface as typed cancellation",
        ));
    }
    Ok(BenchmarkResult {
        backend: "rust-sqlite-vec-production",
        sqlite_vec_version,
        spans: options.spans,
        dimensions: options.dimensions,
        samples: options.samples,
        rebuild_milliseconds: milliseconds(rebuild_duration),
        cold_query_milliseconds: milliseconds(cold_duration),
        warm_query_p50_milliseconds: milliseconds(percentile(&warm_samples, 50)),
        warm_query_p95_milliseconds: milliseconds(percentile(&warm_samples, 95)),
        cancellation_response_microseconds: cancellation_started.elapsed().as_secs_f64() * 1e6,
        candidate_count,
        maximum_candidate_count: TOTAL_CANDIDATES,
        index_bytes,
        executable_bytes: env::current_exe().and_then(std::fs::metadata)?.len(),
        peak_resident_bytes: peak_resident_bytes(),
        private_text_leaves_process: false,
    })
}

fn index_episode(
    index: &mut RecallIndex,
    spans: &[RecallIndexSpan],
    dimensions: usize,
) -> Result<(), RecallIndexError> {
    loop {
        match index.prepare_episode(spans, &index.cancellation())? {
            RecallIndexPlan::Ready { .. } => return Ok(()),
            RecallIndexPlan::NeedsEmbeddings { spans: requested } => {
                let ids = requested
                    .iter()
                    .map(|request| request.span_id)
                    .collect::<Vec<_>>();
                let batch = spans
                    .iter()
                    .filter(|span| ids.contains(&span.span_id))
                    .cloned()
                    .collect::<Vec<_>>();
                let observations = ids
                    .into_iter()
                    .map(|span_id| RecallSpanEmbedding {
                        span_id,
                        embedding: generated_embedding(span_id.low as usize - 1, dimensions),
                    })
                    .collect::<Vec<_>>();
                index.cache_embeddings(&batch, &observations, &index.cancellation())?;
            }
        }
    }
}

fn retrieve(
    index: &RecallIndex,
    query: &RecallEmbeddingVector,
) -> Result<Vec<RecallIndexCandidate>, RecallIndexError> {
    index.retrieve(
        query,
        "needle evidence",
        RecallScope::Library,
        VECTOR_CANDIDATES,
        LEXICAL_CANDIDATES,
        TOTAL_CANDIDATES,
        &index.cancellation(),
    )
}

fn generated_episode(episode_low: u64, count: usize) -> Vec<RecallIndexSpan> {
    (0..count)
        .map(|offset| RecallIndexSpan {
            span_id: EvidenceSpanId::from_parts(100 + episode_low, offset as u64 + 1),
            generation_id: EvidenceGenerationId::from_parts(200, episode_low),
            episode_id: EpisodeId::from_parts(300, episode_low),
            podcast_id: PodcastId::from_parts(400, 1),
            text: if offset == SPANS_PER_EPISODE / 2 {
                format!("needle evidence in representative episode {episode_low}")
            } else {
                format!("background discussion {offset} in episode {episode_low}")
            },
        })
        .collect()
}

fn generated_embedding(offset: usize, dimensions: usize) -> RecallEmbeddingVector {
    let mut values = vec![0; dimensions];
    values[if offset == SPANS_PER_EPISODE / 2 {
        0
    } else {
        (offset + 1) % dimensions
    }] = 1_000_000;
    RecallEmbeddingVector { values }
}

fn query_embedding(dimensions: usize) -> RecallEmbeddingVector {
    let mut values = vec![0; dimensions];
    values[0] = 1_000_000;
    RecallEmbeddingVector { values }
}

fn parse_options() -> Options {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    Options {
        spans: option(&arguments, "--spans", 5_000),
        dimensions: option(&arguments, "--dimensions", 1_024),
        samples: option(&arguments, "--samples", 20),
    }
}

fn option(arguments: &[String], name: &str, default: usize) -> usize {
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .and_then(|pair| pair[1].parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    samples
        .get((samples.len().saturating_sub(1) * percentile) / 100)
        .copied()
        .unwrap_or_default()
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn directory_size(path: &Path) -> Result<u64, std::io::Error> {
    std::fs::read_dir(path)?.try_fold(0_u64, |total, entry| {
        let metadata = entry?.metadata()?;
        Ok(total
            + if metadata.is_file() {
                metadata.len()
            } else {
                0
            })
    })
}

fn peak_resident_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return 0;
    }
    let value = unsafe { usage.assume_init() }.ru_maxrss as u64;
    if cfg!(target_vendor = "apple") {
        value
    } else {
        value.saturating_mul(1_024)
    }
}
