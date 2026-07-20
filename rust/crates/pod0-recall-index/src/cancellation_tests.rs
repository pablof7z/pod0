use std::sync::mpsc;
use std::time::{Duration, Instant};

use pod0_application::{RecallEmbeddingVector, RecallScope};
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};

use crate::{RecallIndex, RecallIndexError, RecallIndexPlan, RecallIndexSpan, RecallSpanEmbedding};

#[test]
fn cancellation_interrupts_an_in_flight_query_within_budget() {
    let span = RecallIndexSpan {
        span_id: EvidenceSpanId::from_parts(1, 1),
        generation_id: EvidenceGenerationId::from_parts(2, 1),
        episode_id: EpisodeId::from_parts(3, 1),
        podcast_id: PodcastId::from_parts(4, 1),
        text: "bounded local recall fixture".to_owned(),
    };
    let query_embedding = RecallEmbeddingVector {
        values: vec![1, 2, 3, 4],
    };
    let mut index = RecallIndex::in_memory(4).unwrap();
    assert!(matches!(
        index
            .prepare_episode(std::slice::from_ref(&span), &index.cancellation())
            .unwrap(),
        RecallIndexPlan::NeedsEmbeddings { .. }
    ));
    index
        .cache_embeddings(
            std::slice::from_ref(&span),
            &[RecallSpanEmbedding {
                span_id: span.span_id,
                embedding: query_embedding.clone(),
            }],
            &index.cancellation(),
        )
        .unwrap();
    assert!(matches!(
        index
            .prepare_episode(std::slice::from_ref(&span), &index.cancellation())
            .unwrap(),
        RecallIndexPlan::Ready { .. }
    ));

    let (started_sender, started_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let mut first_progress = true;
    index
        .connection
        .progress_handler(
            1,
            Some(move || {
                if first_progress {
                    first_progress = false;
                    started_sender.send(()).unwrap();
                    release_receiver.recv().unwrap();
                }
                false
            }),
        )
        .unwrap();

    let cancellation = index.cancellation();
    let query_cancellation = cancellation.clone();
    let query = std::thread::spawn(move || {
        index.retrieve(
            &query_embedding,
            "bounded",
            RecallScope::Library,
            1,
            1,
            2,
            &query_cancellation,
        )
    });
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("query must enter SQLite before cancellation");
    let cancellation_started = Instant::now();
    cancellation.cancel();
    release_sender.send(()).unwrap();
    let result = query.join().unwrap();

    assert!(matches!(result, Err(RecallIndexError::Cancelled)));
    assert!(
        cancellation_started.elapsed() < Duration::from_millis(50),
        "in-flight query cancellation exceeded 50 ms"
    );
}
