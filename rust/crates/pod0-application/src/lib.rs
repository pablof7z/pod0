#![forbid(unsafe_code)]

use pod0_domain::{CommandId, UnixTimestampMilliseconds};

uniffi::setup_scaffolding!();

mod chapter_contract;
#[cfg(test)]
mod chapter_contract_fixture_tests;
#[cfg(test)]
mod chapter_contract_tests;
mod chapter_model_host;
mod chapter_model_policy;
#[cfg(test)]
mod chapter_model_policy_fixture_tests;
mod chapter_model_policy_prompt;
#[cfg(test)]
mod chapter_model_policy_prompt_tests;
mod chapter_model_policy_source;
#[cfg(test)]
mod chapter_model_policy_tests;
mod chapter_model_policy_version;
#[cfg(test)]
mod chapter_model_policy_version_tests;
mod chapter_model_workflow;
#[cfg(test)]
mod chapter_model_workflow_tests;
mod chapter_observation;
mod chapter_observation_agent;
#[cfg(test)]
mod chapter_observation_agent_tests;
mod chapter_observation_model;
#[cfg(test)]
mod chapter_observation_model_tests;
mod chapter_observation_publisher;
#[cfg(test)]
mod chapter_observation_publisher_tests;
#[cfg(test)]
mod chapter_observation_test_support;
mod chapter_observation_values;
mod chapter_projection;
mod chapter_workflow;
mod clip_contract;
mod contract;
mod contract_failure;
mod contract_playback_command;
mod contract_playback_projection;
mod contract_projection;
mod contract_projection_bounds;
#[cfg(test)]
mod contract_projection_tests;
mod contract_state;
mod contract_state_download_validation;
mod contract_state_playback_validation;
#[cfg(test)]
mod contract_state_tests;
mod contract_state_transcript_validation;
mod contract_state_validation;
mod core_wake;
mod download_contract;
#[cfg(test)]
mod download_contract_tests;
mod effects;
mod evidence_contract;
mod feed;
mod feed_parser;
mod feed_parser_reader;
mod feed_parser_values;
#[cfg(test)]
mod feed_tests;
mod host_cancellation;
mod knowledge;
mod knowledge_chunking;
mod knowledge_chunking_policy;
#[cfg(test)]
mod knowledge_chunking_tests;
mod knowledge_ranking;
#[cfg(test)]
mod knowledge_ranking_tests;
#[cfg(test)]
mod knowledge_test_fixture;
mod note_contract;
mod recall_contract;
mod transcript_contract;
#[cfg(test)]
mod transcript_contract_fixture_tests;
#[cfg(test)]
mod transcript_contract_tests;
mod transcript_projection;
mod transcript_workflow;
mod transcript_workflow_capability;
mod transcript_workflow_failure;
mod transcript_workflow_identity;
#[cfg(test)]
mod transcript_workflow_identity_tests;
mod transcript_workflow_policy;
#[cfg(test)]
mod transcript_workflow_tests;

pub use chapter_contract::*;
pub use chapter_model_host::*;
pub use chapter_model_policy::*;
pub use chapter_model_workflow::*;
pub use chapter_observation::*;
pub use chapter_projection::*;
pub use chapter_workflow::*;
pub use clip_contract::*;
pub use contract::*;
pub use contract_failure::*;
pub use contract_playback_command::*;
pub use contract_playback_projection::*;
pub use contract_projection::*;
pub use contract_state::*;
pub use core_wake::*;
pub use download_contract::*;
pub use effects::*;
pub use evidence_contract::*;
pub use feed::*;
pub use host_cancellation::*;
pub use knowledge::*;
pub use knowledge_chunking::*;
pub use knowledge_ranking::*;
pub use note_contract::*;
pub use recall_contract::*;
pub use transcript_contract::*;
pub use transcript_projection::*;
pub use transcript_workflow::*;
pub use transcript_workflow_capability::*;
pub use transcript_workflow_failure::*;
pub use transcript_workflow_identity::*;
pub use transcript_workflow_policy::*;

pub const CORE_SCHEMA_VERSION: u32 = 1;

/// The kernel owns time. Hosts provide an observation through this capability;
/// reducers never sample a native or process-global clock directly.
pub trait Clock: Send + Sync {
    fn now(&self) -> UnixTimestampMilliseconds;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelProbeCommand {
    pub command_id: CommandId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelProbeProjection {
    pub command_id: CommandId,
    pub observed_at: UnixTimestampMilliseconds,
    pub core_schema_version: u32,
}

/// Minimal deterministic application boundary. It persists nothing and emits
/// no host request; the first listening slice will replace the probe with real
/// commands without changing the crate direction or time contract.
pub struct KernelApplication<C> {
    clock: C,
}

impl<C: Clock> KernelApplication<C> {
    #[must_use]
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    #[must_use]
    pub fn dispatch_probe(&self, command: KernelProbeCommand) -> KernelProbeProjection {
        KernelProbeProjection {
            command_id: command.command_id,
            observed_at: self.clock.now(),
            core_schema_version: CORE_SCHEMA_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct FixedClock(UnixTimestampMilliseconds);

    impl Clock for FixedClock {
        fn now(&self) -> UnixTimestampMilliseconds {
            self.0
        }
    }

    #[test]
    fn identical_command_and_time_produce_identical_projection() {
        let time = UnixTimestampMilliseconds::new(1_700_000_000_123);
        let command = KernelProbeCommand {
            command_id: CommandId::from_bytes([9; 16]),
        };

        let first = KernelApplication::new(FixedClock(time)).dispatch_probe(command);
        let second = KernelApplication::new(FixedClock(time)).dispatch_probe(command);

        assert_eq!(first, second);
        assert_eq!(first.observed_at, time);
        assert_eq!(first.core_schema_version, CORE_SCHEMA_VERSION);
    }
}
