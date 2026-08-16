#![forbid(unsafe_code)]

uniffi::setup_scaffolding!();

mod activity_contract;
mod activity_execution_contract;
mod activity_identity;
mod activity_routing_command;
mod activity_routing_effect;
mod activity_routing_observation;
mod activity_transition_kind;
mod agent_action_hash;
mod agent_action_hash_primitives;
mod agent_action_validation;
mod agent_activity;
mod agent_activity_identity;
mod agent_contract;
mod agent_execution_activity;
mod agent_generated_audio;
mod agent_history_contract;
mod agent_history_import;
mod agent_policy;
mod agent_policy_shape;
mod agent_provider_output;
mod agent_recall_activity;
mod agent_recall_contract;
mod agent_run_contract;
mod agent_tool_catalog;
mod agent_tool_catalog_builders;
mod agent_tool_catalog_discovery;
mod agent_tool_names;
#[cfg(test)]
mod agent_tool_policy_tests;
mod agent_turn_contract;
mod agent_workflow;
#[cfg(test)]
mod agent_workflow_compatibility_tests;
mod agent_workflow_continuation;
mod agent_workflow_recovery;
#[cfg(test)]
mod agent_workflow_tests;
mod agent_workflow_values;
mod cancellation_activity;
mod chapter_artifact_activity;
mod chapter_contract;
mod chapter_cutover_activity;
mod chapter_finalization_activity;
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
mod chapter_observation_activity;
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
mod contract_library_input;
mod contract_operation;
mod contract_playback_command;
mod contract_playback_projection;
mod contract_projection;
mod contract_projection_bounds;
mod contract_state;
mod contract_state_agent_validation;
mod contract_state_download_validation;
mod contract_state_playback_validation;
mod contract_state_scheduled_agent_validation;
mod contract_state_subscription;
#[cfg(test)]
mod contract_state_tests;
mod contract_state_transcript_validation;
mod contract_state_validation;
mod core_wake;
mod download_activity;
mod download_contract;
#[cfg(test)]
mod download_contract_tests;
mod download_control_activity;
mod download_cutover_activity;
mod download_disposition_activity;
mod download_effect_contract;
mod download_environment_activity;
mod download_finalization_activity;
mod download_observation_activity;
mod download_recovery_activity;
mod effect_lease_contract;
mod effects;
mod episode_web_metadata;
mod episode_web_metadata_entities;
mod episode_web_metadata_html;
#[cfg(test)]
mod episode_web_metadata_tests;
mod evidence_activity;
mod evidence_contract;
mod evidence_observation_activity;
mod evidence_rebuild_activity;
mod internal_command_owner_activity;
mod legacy_business_cutover_activity;
mod library_catalog;
#[cfg(test)]
mod library_catalog_tests;
mod library_directory;
#[cfg(test)]
mod library_directory_tests;
mod library_network_activity;
mod library_network_contract;
mod lifecycle_activity;
mod lifecycle_effect_contract;
include!("feed_activity_modules.rs");
mod feed_parser;
mod feed_parser_reader;
mod feed_parser_values;
#[cfg(test)]
mod feed_tests;
mod host_cancellation;
mod kernel_probe;
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
mod library_activity;
mod library_command_activity;
mod library_feed_migration_activity;
mod listening_reset_activity;
mod memory_contract;
mod migration_activity;
include!("note_activity_modules.rs");
mod playback_activity;
mod playback_effect_contract;
mod playback_observation_activity;
mod publication;
mod recall_configuration_activity;
mod recall_contract;
mod recall_workflow_activity;
mod recall_workflow_contract;
mod request_disposition_activity;
mod scheduled_agent;
mod scheduled_agent_completion;
#[cfg(test)]
mod scheduled_agent_host_ledger_tests;
mod scheduled_agent_observation;
mod scheduled_agent_observation_validation;
mod scheduled_agent_policy;
#[cfg(test)]
mod scheduled_agent_tests;
mod shared_episode_resolution;
mod speaker_activity;
mod transcript_activity;
#[cfg(test)]
mod transcript_activity_tests;
mod transcript_admission_activity;
#[cfg(test)]
mod transcript_admission_activity_tests;
mod transcript_artifact_activity;
mod transcript_cancellation_activity;
#[cfg(test)]
mod transcript_cancellation_activity_tests;
mod transcript_contract;
#[cfg(test)]
mod transcript_contract_fixture_tests;
#[cfg(test)]
mod transcript_contract_tests;
mod transcript_cutover_activity;
mod transcript_finalization_activity;
mod transcript_observation_activity;
#[cfg(test)]
mod transcript_observation_activity_tests;
mod transcript_observation_policy;
#[cfg(test)]
mod transcript_observation_policy_tests;
mod transcript_projection;
mod transcript_recovery_activity;
mod transcript_workflow;
mod transcript_workflow_capability;
mod transcript_workflow_failure;
mod transcript_workflow_identity;
#[cfg(test)]
mod transcript_workflow_identity_tests;
mod transcript_workflow_policy;
#[cfg(test)]
mod transcript_workflow_projection_tests;
#[cfg(test)]
mod transcript_workflow_tests;
mod transition_plan;
mod workflow_action;
mod workflow_cancellation_activity;
mod workflow_configuration;
mod workflow_configuration_activity;
mod workflow_reconcile;
mod workflow_reconcile_activity;
#[cfg(test)]
mod workflow_reconcile_activity_tests;
#[cfg(test)]
mod workflow_reconcile_tests;

#[cfg(test)]
include!("activity_test_modules.rs");

include!("application_exports.rs");
