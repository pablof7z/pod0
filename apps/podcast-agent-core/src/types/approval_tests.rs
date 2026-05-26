use super::*;


fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
}

#[test]
fn pending_approval_round_trips() {
    let a = PendingApproval {
        id: Uuid::nil(),
        conversation_id: Uuid::nil(),
        action_description: "publish clip".into(),
        requested_at: ts(1_700_000_000),
        expires_at: Some(ts(1_700_001_000)),
    };
    let j = serde_json::to_string(&a).expect("encode");
    let d: PendingApproval = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, a);
}

#[test]
fn pending_approval_omits_none_expiry() {
    let a = PendingApproval::new(Uuid::nil(), "noop");
    let j = serde_json::to_string(&a).expect("encode");
    assert!(!j.contains("\"expires_at\""));
}

#[test]
fn approval_decision_serde_tag() {
    let approved = ApprovalDecision::Approved;
    let j = serde_json::to_string(&approved).expect("encode");
    assert_eq!(j, r#"{"kind":"approved"}"#);

    let denied = ApprovalDecision::Denied {
        reason: Some("too risky".into()),
    };
    let j = serde_json::to_string(&denied).expect("encode");
    assert_eq!(j, r#"{"kind":"denied","reason":"too risky"}"#);

    let denied_none = ApprovalDecision::Denied { reason: None };
    let j = serde_json::to_string(&denied_none).expect("encode");
    assert_eq!(j, r#"{"kind":"denied"}"#);
}

#[test]
fn approval_decision_round_trips() {
    let cases = [
        ApprovalDecision::Approved,
        ApprovalDecision::Denied {
            reason: Some("nope".into()),
        },
        ApprovalDecision::Denied { reason: None },
    ];
    for c in cases {
        let j = serde_json::to_string(&c).expect("encode");
        let d: ApprovalDecision = serde_json::from_str(&j).expect("decode");
        assert_eq!(d, c);
    }
}

#[test]
fn expiry_check_uses_supplied_clock() {
    let mut a = PendingApproval::new(Uuid::nil(), "noop");
    a.expires_at = Some(ts(1_700_000_000));
    assert!(!a.is_expired_at(ts(1_699_999_999)));
    assert!(a.is_expired_at(ts(1_700_000_000)));
    assert!(a.is_expired_at(ts(1_700_000_001)));

    a.expires_at = None;
    // No expiry → never expires, even far in the future. We pick a
    // very-large-but-valid chrono timestamp (year ~4000 fits well
    // inside the supported range).
    assert!(!a.is_expired_at(ts(64_060_588_800)));
}
