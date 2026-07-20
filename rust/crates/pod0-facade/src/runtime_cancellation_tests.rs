use std::sync::Arc;
use std::time::{Duration, Instant};

use pod0_application::{ApplicationCommand, CommandEnvelope};
use pod0_domain::{CancellationId, CommandId};
use pod0_recall_index::RecallCancellation;

use crate::Pod0Facade;

#[test]
fn facade_cancel_signals_active_recall_before_waiting_for_state_lock() {
    let facade = Pod0Facade::new();
    let cancellation_id = CancellationId::from_parts(81, 1);
    let lease = facade
        .recall_interrupts
        .begin(cancellation_id, RecallCancellation::default());
    let state_guard = facade.state();
    let cancellation_facade = Arc::clone(&facade);
    let cancellation_thread = std::thread::spawn(move || {
        cancellation_facade.dispatch(CommandEnvelope {
            command_id: CommandId::from_parts(80, 2),
            cancellation_id: CancellationId::from_parts(81, 2),
            expected_revision: None,
            command: ApplicationCommand::CancelOperation { cancellation_id },
        });
    });

    let deadline = Instant::now() + Duration::from_millis(50);
    while !lease.cancellation().is_cancelled() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(
        lease.cancellation().is_cancelled(),
        "facade cancellation waited for the state lock"
    );

    drop(state_guard);
    cancellation_thread.join().unwrap();
}
