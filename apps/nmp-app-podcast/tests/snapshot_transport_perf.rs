//! Transport-layer measurement for the podcast library snapshot.
//!
//! NOT a pass/fail test — a measurement harness (run with `--nocapture`). It
//! isolates the FFI *transport* cost the iOS shell pays on every content tick:
//! serialize the full `PodcastUpdate` → JSON (Rust), and the bytes that then
//! cross the FFI and get JSON-decoded in Swift. It deliberately does NOT model
//! the `build_podcast_update` field-mapping / `clean_html` cost (that's the
//! `fix/snapshot-strip-html-memo` lane) — only the serialize+payload+decode the
//! delta-projection idea targets.
//!
//! Run: `cargo test -p nmp-app-podcast --test snapshot_transport_perf -- --nocapture`

use std::time::Instant;

use nmp_app_podcast::ffi::projections::{EpisodeSummary, PodcastSummary};
use nmp_app_podcast::ffi::PodcastUpdate;

/// A realistic show-notes blob. Real podcast descriptions routinely run several
/// hundred bytes of prose (links, sponsor reads, chapter lists). ~640 bytes.
const DESCRIPTION: &str = "In this episode we sit down with our guest to unpack the \
week's biggest stories, dig into the research behind the headlines, and answer \
listener questions from the mailbag. We cover the new findings, what they mean for \
you, and where the experts disagree. Plus: a lightning round, a few tangents, and \
our picks of the week. Show notes, links, and the full transcript are available on \
our website. This episode is brought to you by our sponsors — visit the links in \
the description to support the show and get a discount on your first order.";

/// A realistic AI summary (2-3 sentences). ~230 bytes.
const SUMMARY: &str = "The hosts interview a guest about recent developments, \
covering the key findings and their practical implications. They debate points of \
expert disagreement and close with a lightning round and weekly picks.";

fn make_episode(podcast_id: &str, podcast_title: &str, i: usize) -> EpisodeSummary {
    EpisodeSummary {
        id: format!("a1b2c3d4-e5f6-4a7b-8c9d-{:012x}", i),
        title: format!("Episode {i}: A Reasonably Long Human-Readable Episode Title"),
        podcast_id: Some(podcast_id.to_string()),
        podcast_title: Some(podcast_title.to_string()),
        duration_secs: Some(3600.0 + i as f64),
        artwork_url: Some(format!(
            "https://cdn.example.com/artwork/{podcast_id}/episode-{i}-1400x1400.jpg"
        )),
        published_at: Some(1_700_000_000 + i as i64 * 86_400),
        enclosure_url: Some(format!(
            "https://traffic.example.com/podcast/{podcast_id}/episode-{i}.mp3?token=abc123"
        )),
        description: Some(DESCRIPTION.to_string()),
        // Roughly a third of episodes carry an AI summary; none carry a full
        // transcript (transcripts are fetched on demand, not in the library).
        summary: if i % 3 == 0 { Some(SUMMARY.to_string()) } else { None },
        played: i % 4 == 0,
        ..Default::default()
    }
}

fn make_library(num_shows: usize, eps_per_show: usize) -> Vec<PodcastSummary> {
    (0..num_shows)
        .map(|s| {
            let id = format!("f0e1d2c3-b4a5-4968-8778-{:012x}", s);
            let title = format!("The Reasonably Named Podcast Number {s}");
            let episodes = (0..eps_per_show)
                .map(|i| make_episode(&id, &title, s * eps_per_show + i))
                .collect();
            PodcastSummary {
                id,
                title,
                episode_count: eps_per_show,
                unplayed_count: eps_per_show * 3 / 4,
                artwork_url: Some(format!("https://cdn.example.com/shows/{s}/cover-3000x3000.jpg")),
                feed_url: Some(format!("https://feeds.example.com/show-{s}/rss.xml")),
                author: Some("A Reasonably Named Production Company, LLC".to_string()),
                description: Some(DESCRIPTION.to_string()),
                episodes,
                ..Default::default()
            }
        })
        .collect()
}

fn median_micros<F: FnMut()>(iters: u32, mut f: F) -> u128 {
    let mut samples: Vec<u128> = (0..iters)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_micros()
        })
        .collect();
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
fn measure_full_library_transport() {
    println!("\n=== Full-library snapshot transport (serialize + payload bytes + decode) ===");
    println!(
        "{:>8} {:>9} {:>12} {:>14} {:>16}",
        "shows", "eps/show", "total eps", "payload (KB)", ""
    );
    println!(
        "{:>8} {:>9} {:>12} {:>14} {:>10} {:>10}",
        "", "", "", "", "ser µs", "de µs"
    );

    for &(shows, per) in &[(20usize, 50usize), (20, 180), (20, 500), (20, 1000)] {
        let library = make_library(shows, per);
        let total: usize = library.iter().map(|p| p.episodes.len()).sum();
        let update = PodcastUpdate { library, ..PodcastUpdate::default() };

        // Warm + correctness: it must round-trip.
        let json = serde_json::to_string(&update).expect("serialize");
        let _decoded: PodcastUpdate = serde_json::from_str(&json).expect("deserialize");

        let ser_us = median_micros(11, || {
            let _ = serde_json::to_string(&update).unwrap();
        });
        let de_us = median_micros(11, || {
            let _: PodcastUpdate = serde_json::from_str(&json).unwrap();
        });

        println!(
            "{:>8} {:>9} {:>12} {:>14.1} {:>10} {:>10}",
            shows,
            per,
            total,
            json.len() as f64 / 1024.0,
            ser_us,
            de_us
        );
    }
    println!();
}

#[test]
fn measure_single_field_change_amplification() {
    // The core thesis: flipping ONE field on ONE episode (mark-played) forces a
    // re-serialize + re-decode of the WHOLE library, because the wire contract
    // re-emits everything. Quantify the amplification: full-library payload vs.
    // the bytes that actually changed (one episode row).
    println!("\n=== Single-field-change amplification (mark-played on 1 episode) ===");

    let library = make_library(20, 180); // ~3,600 eps — the flagged worst case
    let total: usize = library.iter().map(|p| p.episodes.len()).sum();
    let update = PodcastUpdate { library, ..PodcastUpdate::default() };

    let full_json = serde_json::to_string(&update).expect("serialize full");
    let full_kb = full_json.len() as f64 / 1024.0;

    // What a narrow "changed rows" projection would actually need to ship: the
    // single episode that changed.
    let one = make_episode("f0e1d2c3-b4a5-4968-8778-000000000000", "Show", 0);
    let one_json = serde_json::to_string(&one).expect("serialize one");
    let one_b = one_json.len();

    let full_ser = median_micros(11, || {
        let _ = serde_json::to_string(&update).unwrap();
    });
    let full_de = median_micros(11, || {
        let _: PodcastUpdate = serde_json::from_str(&full_json).unwrap();
    });

    println!("  library size:                 {total} episodes");
    println!("  FULL snapshot payload:        {full_kb:.1} KB");
    println!("  FULL serialize (Rust):        {full_ser} µs");
    println!("  FULL deserialize (serde):     {full_de} µs  (Swift JSONDecoder is slower)");
    println!("  ONE changed episode row:      {one_b} bytes");
    println!(
        "  amplification factor:         {:.0}x bytes re-shipped for a 1-field change",
        full_json.len() as f64 / one_b as f64
    );
    println!();
}
