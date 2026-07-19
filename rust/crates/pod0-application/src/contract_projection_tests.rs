use crate::{MAX_PROJECTION_ITEMS, OperationStage, ProjectionRequest, ProjectionScope};

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
