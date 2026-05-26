use super::*;


#[test]
fn memory_kind_default_is_fact() {
    assert_eq!(MemoryKind::default(), MemoryKind::Fact);
}

#[test]
fn memory_kind_round_trips() {
    let cases = [
        MemoryKind::Fact,
        MemoryKind::Preference,
        MemoryKind::Routine,
        MemoryKind::Note,
    ];
    for k in cases {
        let j = serde_json::to_string(&k).expect("encode");
        let d: MemoryKind = serde_json::from_str(&j).expect("decode");
        assert_eq!(d, k);
    }
}

#[test]
fn agent_memory_round_trips() {
    let m = AgentMemory {
        id: Uuid::nil(),
        content: "user likes 1.5x".into(),
        created_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        deleted: false,
        kind: MemoryKind::Preference,
    };
    let j = serde_json::to_string(&m).expect("encode");
    let d: AgentMemory = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, m);
}

#[test]
fn agent_memory_missing_kind_decodes_as_fact() {
    // Forward-compat with legacy persisted JSON predating `kind`.
    let payload = r#"{"id":"00000000-0000-0000-0000-000000000000","content":"x","created_at":"2024-01-01T00:00:00Z","deleted":false}"#;
    let d: AgentMemory = serde_json::from_str(payload).expect("decode");
    assert_eq!(d.kind, MemoryKind::Fact);
}

#[test]
fn is_active_tracks_deleted_flag() {
    let mut m = AgentMemory::new(MemoryKind::Fact, "x");
    assert!(m.is_active());
    m.deleted = true;
    assert!(!m.is_active());
}
