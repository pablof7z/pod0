use crate::{
    MAX_PROJECTION_BATCH_ITEMS, MAX_PROJECTION_ITEMS, OperationStage, ProjectionBatchRequest,
    ProjectionRequest, ProjectionScope,
};

#[test]
fn projection_requests_are_bounded_and_terminal_stages_are_explicit() {
    let empty = ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 0,
    };
    let oversized = ProjectionRequest {
        scope: ProjectionScope::Playback,
        offset: u32::MAX,
        max_items: u16::MAX,
    };
    assert_eq!(empty.bounded_max_items(), 1);
    assert_eq!(
        oversized.bounded_max_items(),
        usize::from(MAX_PROJECTION_ITEMS)
    );
    assert!(!OperationStage::Accepted.is_terminal());
    assert!(OperationStage::Failed.is_terminal());
    assert!(OperationStage::Unsupported { wire_code: 99 }.is_terminal());
}

#[test]
fn projection_batches_are_bounded_without_changing_request_order() {
    let requests = (0..=MAX_PROJECTION_BATCH_ITEMS)
        .map(|offset| ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: u32::from(offset),
            max_items: 1,
        })
        .collect::<Vec<_>>();
    let batch = ProjectionBatchRequest { requests };
    assert_eq!(
        batch.bounded_requests().len(),
        usize::from(MAX_PROJECTION_BATCH_ITEMS)
    );
    assert!(batch.has_more());
    assert_eq!(batch.bounded_requests()[0].offset, 0);
    assert_eq!(
        batch.bounded_requests().last().unwrap().offset,
        u32::from(MAX_PROJECTION_BATCH_ITEMS - 1)
    );
}
