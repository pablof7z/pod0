use pod0_facade::{
    AgentCapabilityOutcome, AgentCapabilityRequest, AgentToolAction, AgentToolName, HostObservation,
};

use super::{HostExecutor, search};
use crate::protocol::{CliError, PodcastSearchResultDto};

/// Executes an agent-proposed capability headlessly. Only
/// `AgentToolAction::Search{tool: SearchPodcastDirectory, ..}` is fully
/// replicable outside the iOS simulator (pure HTTP, no native audio engine
/// dependency); every other action returns a specific `Failed` outcome
/// naming the unsupported action. `execute_first` is read but intentionally
/// ignored — there is no headless audio engine to auto-play a result into,
/// so a search always returns the result set only, never enqueues playback.
pub(crate) fn execute(host: &HostExecutor, capability: &AgentCapabilityRequest) -> HostObservation {
    let outcome = match &capability.action {
        AgentToolAction::Search {
            tool: AgentToolName::SearchPodcastDirectory,
            query,
            limit,
            ..
        } => search_outcome(search::search(host, query, *limit)),
        _ => AgentCapabilityOutcome::Failed {
            safe_detail: Some(format!(
                "agent capability '{:?}' is unsupported in the headless host",
                capability.action
            )),
        },
    };
    HostObservation::AgentCapabilityObserved {
        turn_id: capability.turn_id,
        proposal_id: capability.proposal_id,
        execution_fence_id: capability.execution_fence_id,
        outcome,
    }
}

fn search_outcome(result: Result<Vec<PodcastSearchResultDto>, CliError>) -> AgentCapabilityOutcome {
    match result {
        Ok(results) => AgentCapabilityOutcome::Succeeded {
            bounded_result: bounded_result_json(results),
        },
        Err(error) => AgentCapabilityOutcome::Failed {
            safe_detail: Some(error.message),
        },
    }
}

/// Serializes search results to JSON, dropping whole trailing entries (never
/// truncating mid-object) until the result fits within
/// `pod0_application::MAX_AGENT_MESSAGE_BYTES` — the durable-core observation
/// bound, independent of and tighter than `search.rs`'s own HTTP-layer
/// `MAX_SEARCH_RESULTS`/`MAX_SEARCH_RESPONSE_BYTES` bounds.
fn bounded_result_json(mut results: Vec<PodcastSearchResultDto>) -> String {
    loop {
        let serialized = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_owned());
        if serialized.len() <= pod0_application::MAX_AGENT_MESSAGE_BYTES || results.is_empty() {
            return serialized;
        }
        results.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::HostConfig;

    fn play_episode_capability() -> AgentCapabilityRequest {
        AgentCapabilityRequest {
            turn_id: pod0_domain::AgentTurnId::from_parts(1, 1),
            proposal_id: pod0_domain::AgentProposalId::from_parts(1, 1),
            proposal_digest: pod0_domain::ContentDigest::from_bytes([0; 32]),
            execution_fence_id: pod0_domain::AgentExecutionFenceId::from_parts(1, 1),
            execution_mode: pod0_facade::AgentCapabilityExecutionMode::Perform,
            generated_audio_target: None,
            action: AgentToolAction::PlayEpisode {
                episode_id: pod0_domain::EpisodeId::from_parts(1, 1),
                start_milliseconds: None,
                end_milliseconds: None,
                placement: pod0_facade::QueuePlacement::Now,
            },
        }
    }

    #[test]
    fn search_podcast_directory_succeeds_with_a_short_result_set() {
        let outcome = search_outcome(Ok(vec![PodcastSearchResultDto {
            itunes_id: 42,
            title: "Tech Talk".to_owned(),
            author: "Someone".to_owned(),
            feed_url: Some("https://example.com/feed".to_owned()),
            artwork_url: None,
            track_count: Some(5),
        }]));
        assert!(matches!(outcome, AgentCapabilityOutcome::Succeeded { .. }));
    }

    #[test]
    fn every_non_search_capability_action_fails_with_a_specific_message() {
        let host = HostExecutor::new(HostConfig::empty()).expect("host executor constructs");
        let capability = play_episode_capability();
        let observation = execute(&host, &capability);
        let HostObservation::AgentCapabilityObserved { outcome, .. } = observation else {
            panic!("expected AgentCapabilityObserved");
        };
        match outcome {
            AgentCapabilityOutcome::Failed { safe_detail } => {
                let detail = safe_detail.expect("PlayEpisode must carry a specific detail");
                assert!(detail.contains("PlayEpisode"));
                assert_ne!(
                    detail,
                    "agent capability execution is unavailable in the headless host"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn two_hundred_result_response_is_clamped_below_the_bound() {
        let long_string = "x".repeat(400);
        let results: Vec<PodcastSearchResultDto> = (0..200)
            .map(|index| PodcastSearchResultDto {
                itunes_id: index,
                title: format!("{long_string}-{index}"),
                author: long_string.clone(),
                feed_url: Some(format!("https://example.com/{long_string}/{index}")),
                artwork_url: Some(format!("https://example.com/{long_string}/art/{index}")),
                track_count: Some(10),
            })
            .collect();
        let naive = serde_json::to_string(&results).unwrap();
        assert!(
            naive.len() > pod0_application::MAX_AGENT_MESSAGE_BYTES,
            "test fixture must exceed the bound to exercise the clamp"
        );
        let bounded = bounded_result_json(results);
        assert!(bounded.len() <= pod0_application::MAX_AGENT_MESSAGE_BYTES);
        let parsed: serde_json::Value = serde_json::from_str(&bounded).unwrap();
        let entries = parsed.as_array().expect("bounded_result must be a JSON array");
        assert!(!entries.is_empty());
        assert!(entries.len() < 200);
    }
}
