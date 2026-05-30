use super::*;
#[test]
fn stub_returns_ok_envelope_with_generating_status() {
    let slot = Arc::new(Mutex::new(None));
    let rev = Arc::new(AtomicU64::new(0));
    let v = handle_generate_briefing(&slot, &rev, None, None);
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "generating");
}
#[test]
fn stub_writes_generating_snapshot_and_bumps_rev() {
    let slot = Arc::new(Mutex::new(None));
    let rev = Arc::new(AtomicU64::new(7));
    let _ = handle_generate_briefing(&slot, &rev, None, None);
    let stored = slot.lock().unwrap().clone().expect("briefing slot populated");
    assert_eq!(stored.status, "generating");
    assert!(stored.is_generating);
    assert!(stored.segments.is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 8);
}

