use super::*;
fn cand(ep: &str, pod_id: &str, pod_title: &str, ts: i64) -> CandidateEpisode {
    CandidateEpisode {
        episode_id: ep.into(),
        episode_title: format!("{ep} title"),
        podcast_id: pod_id.into(),
        podcast_title: pod_title.into(),
        artwork_url: None,
        published_at: ts,
        duration_secs: Some(1800.0),
    }
}

/// Test helper: extract `(action_json, correlation_id)` from an
/// `ActorCommand::Protocol(HostOpCommand { .. })` via its `Debug` output.
/// HostOpCommand fields are private in nmp-core; this avoids direct access.
#[cfg(test)]
#[allow(dead_code)]
fn extract_host_op_parts(cmd: &ActorCommand) -> (String, String) {
    let dbg = format!("{cmd:?}");
    // Debug fmt: Protocol(HostOpCommand { action_json: "{..}", correlation_id: "corr" })
    // The outer string delimiters are literal " in the Debug output; inner " are \".
    let jm = concat!("action_json: ", r#"""#);
    let js = dbg.find(jm).expect("action_json") + jm.len();
    let after = &dbg[js..];
    let je = after.find(concat!(r#"""#, ", correlation_id:")).expect("json end");
    let raw = &after[..je];
    // Unescape \" → " and \\\\ → \\
    let tmp = raw.replace(r#"\\"#, "\x01BSLASH\x01");
    let action_json = tmp.replace(r#"\""#, r#"""#).replace("\x01BSLASH\x01", "\\");
    let cm = concat!("correlation_id: ", r#"""#);
    let cs = dbg.find(cm).expect("corr_id") + cm.len();
    let after_c = &dbg[cs..];
    let ce = after_c.find(concat!(r#"""#, " }")).expect("corr end");
    (action_json, after_c[..ce].to_string())
}

#[test]
fn refresh_action_round_trips() {
    let a = PicksAction::Refresh;
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, r#"{"op":"refresh"}"#);
    let decoded: PicksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn namespace_is_podcast_picks() {
    assert_eq!(AgentPicksModule::NAMESPACE, "podcast.picks");
}
#[test]
fn execute_emits_dispatch_host_op() {
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    AgentPicksModule.execute(PicksAction::Refresh, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0]
    else { panic!("expected Protocol command"); };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-1");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast.picks");
    assert_eq!(v["action"]["op"], "refresh");
}
#[test]
fn compute_picks_empty_input_returns_empty() {
    let picks = compute_picks(vec![]);
    assert!(picks.is_empty());
}
#[test]
fn compute_picks_orders_newest_first() {
    let candidates = vec![
        cand("ep-old", "pod-1", "Show A", 1_000),
        cand("ep-new", "pod-2", "Show B", 9_000),
        cand("ep-mid", "pod-3", "Show C", 5_000),
    ];
    let picks = compute_picks(candidates);
    assert_eq!(picks.len(), 3);
    assert_eq!(picks[0].episode_id, "ep-new");
    assert_eq!(picks[1].episode_id, "ep-mid");
    assert_eq!(picks[2].episode_id, "ep-old");
}
#[test]
fn compute_picks_caps_per_show() {
    // Five episodes from the same show — only PICKS_PER_SHOW_CAP (2)
    // should make it through.
    let candidates: Vec<CandidateEpisode> = (0..5)
        .map(|i| cand(&format!("ep-{i}"), "pod-mono", "Daily Show", 100 + i as i64))
        .collect();
    let picks = compute_picks(candidates);
    assert_eq!(picks.len(), PICKS_PER_SHOW_CAP);
    // The two newest episodes from the mono-show should win.
    assert_eq!(picks[0].episode_id, "ep-4");
    assert_eq!(picks[1].episode_id, "ep-3");
}
#[test]
fn compute_picks_caps_total_at_limit() {
    // 20 episodes across 20 shows — exactly PICKS_LIMIT (10) survive.
    let candidates: Vec<CandidateEpisode> = (0..20)
        .map(|i| {
            cand(
                &format!("ep-{i}"),
                &format!("pod-{i}"),
                &format!("Show {i}"),
                100 + i as i64,
            )
        })
        .collect();
    let picks = compute_picks(candidates);
    assert_eq!(picks.len(), PICKS_LIMIT);
}
#[test]
fn compute_picks_assigns_descending_scores() {
    let candidates = vec![
        cand("ep-1", "pod-1", "A", 300),
        cand("ep-2", "pod-2", "B", 200),
        cand("ep-3", "pod-3", "C", 100),
    ];
    let picks = compute_picks(candidates);
    assert_eq!(picks.len(), 3);
    assert!((picks[0].pick_score - 1.0).abs() < 1e-6);
    assert!(picks[0].pick_score > picks[1].pick_score);
    assert!(picks[1].pick_score > picks[2].pick_score);
    // Even the last pick must be positive.
    assert!(picks.last().unwrap().pick_score > 0.0);
}
#[test]
fn compute_picks_sets_reason_with_podcast_title() {
    let candidates = vec![cand("ep-1", "pod-1", "Stratechery", 1_000)];
    let picks = compute_picks(candidates);
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0].pick_reason, "New from Stratechery");
}
#[test]
fn compute_picks_diversity_across_three_shows() {
    // A high-frequency show with 5 episodes, plus two single-episode
    // shows. The mono show contributes 2 (cap), the others 1 each.
    let mut candidates: Vec<CandidateEpisode> = (0..5)
        .map(|i| cand(&format!("daily-{i}"), "pod-daily", "Daily", 100 + i as i64))
        .collect();
    candidates.push(cand("solo-a", "pod-a", "Show A", 50));
    candidates.push(cand("solo-b", "pod-b", "Show B", 40));
    let picks = compute_picks(candidates);
    assert_eq!(picks.len(), 4); // 2 daily + 2 solo
    let daily_count = picks.iter().filter(|p| p.podcast_id == "pod-daily").count();
    assert_eq!(daily_count, PICKS_PER_SHOW_CAP);
}
