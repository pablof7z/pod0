use std::time::{Duration, Instant};

use crate::*;

fn request(scope: ProjectionScope) -> ProjectionRequest {
    ProjectionRequest {
        scope,
        offset: 0,
        max_items: u16::MAX,
    }
}

fn baseline_batch() -> ProjectionBatchRequest {
    ProjectionBatchRequest {
        requests: vec![
            request(ProjectionScope::Library),
            request(ProjectionScope::Playback),
            request(ProjectionScope::Notes {
                scope: NoteProjectionScope::All,
            }),
            request(ProjectionScope::Memories {
                scope: MemoryProjectionScope::Active,
            }),
        ],
    }
}

#[test]
fn batch_snapshot_is_revision_consistent_and_bounded_on_both_axes() {
    let facade = Pod0Facade::new();
    let request = ProjectionBatchRequest {
        requests: (0..=MAX_PROJECTION_BATCH_ITEMS)
            .map(|_| request(ProjectionScope::Library))
            .collect(),
    };
    let batch = facade.snapshot_batch(request);
    assert!(batch.has_more);
    assert_eq!(
        batch.projections.len(),
        usize::from(MAX_PROJECTION_BATCH_ITEMS)
    );
    assert!(
        batch
            .projections
            .iter()
            .all(|projection| projection.state_revision == batch.state_revision)
    );
    for envelope in batch.projections {
        let Projection::Library { value } = envelope.projection else {
            panic!("expected library projection")
        };
        assert!(value.episodes.len() <= usize::from(MAX_PROJECTION_ITEMS));
        assert!(value.podcasts.len() <= usize::from(MAX_PROJECTION_ITEMS));
    }
}

#[test]
fn baseline_projection_batch_stays_within_documented_budget() {
    let facade = Pod0Facade::new();
    for _ in 0..3 {
        assert_eq!(facade.snapshot_batch(baseline_batch()).projections.len(), 4);
    }
    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let started = Instant::now();
        let batch = facade.snapshot_batch(baseline_batch());
        samples.push(started.elapsed());
        assert_eq!(batch.projections.len(), 4);
    }
    samples.sort_unstable();
    let median = samples[10];
    let p95 = samples[18];
    eprintln!("projection_batch_4_median={median:?} p95={p95:?}");
    let budget = if cfg!(debug_assertions) {
        Duration::from_millis(250)
    } else {
        Duration::from_millis(25)
    };
    assert!(
        p95 < budget,
        "projection batch p95 {p95:?} exceeded {budget:?}"
    );
}
