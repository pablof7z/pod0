/// Every table introduced by a migration after schema 13, up through
/// `pod0_recall_configuration` (schema 18). Migration tests fake an older
/// store by migrating to `CURRENT_SCHEMA_VERSION` and then hand-reverting the
/// tables a later schema step would have added, before stamping the version
/// number back down and re-running the real migration path.
///
/// This block is shared verbatim across every such fixture: whichever schema
/// version a given test reverts *to*, it always needs to drop everything
/// newer than 13 up to that floor, and every one of those fixtures happens to
/// need at least this much dropped. Callers append whatever else their target
/// version also requires before it.
pub(crate) const TABLES_ADDED_AFTER_V13: &str = "
    DROP TABLE pod0_category_members;
    DROP TABLE pod0_categories;
    DROP TABLE pod0_category_state;
    DROP TABLE pod0_feed_discovery_cutover_candidates;
    DROP TABLE pod0_feed_discovery_cutover;
    DROP TABLE pod0_feed_apply_receipts;
    DROP TABLE pod0_feed_discovery_effects;
    DROP TABLE pod0_feed_discovery_workflows;
    DROP TABLE pod0_new_episode_notification_settings;
    DROP TABLE pod0_feed_discovery_items;
    DROP TABLE pod0_feed_discovery_occurrences;
    DROP TABLE pod0_compiled_memory_sources;
    DROP TABLE pod0_compiled_memory;
    DROP TABLE pod0_memories;
    DROP TABLE pod0_memory_cutover_evidence;
    DROP TABLE pod0_memory_state;
    DROP TABLE pod0_agent_history_staged_turns;
    DROP TABLE pod0_agent_history_staged_conversations;
    DROP TABLE pod0_agent_history_cutover_evidence;
    DROP TABLE pod0_agent_conversation_metadata;
    DROP TABLE pod0_publication_commands;
    DROP TABLE pod0_publication_facts;
    DROP TABLE pod0_signer_state;
    DROP TABLE pod0_publications;
    DROP TABLE pod0_agent_generated_audio_artifacts;
    DROP TABLE pod0_agent_audit;
    DROP TABLE pod0_agent_command_receipts;
    DROP TABLE pod0_agent_turns;
    DROP TABLE pod0_scheduled_completion_evidence;
    DROP TABLE pod0_generated_artifacts;
    DROP TABLE pod0_scheduled_command_receipts;
    DROP TABLE pod0_scheduled_attempts;
    DROP TABLE pod0_scheduled_occurrences;
    DROP TABLE pod0_scheduled_tasks;
    DROP TABLE pod0_scheduled_agent_cutover_evidence;
    DROP TABLE pod0_scheduled_agent_authority;
    DROP TABLE pod0_transcript_evidence_requests;
    DROP TABLE pod0_transcript_attempts;
    DROP TABLE pod0_transcript_workflows;
    DROP TABLE pod0_transcript_workflow_import_rows;
    DROP TABLE pod0_transcript_workflow_imports;
    DROP TABLE pod0_download_host_requests;
    DROP TABLE pod0_download_attempts;
    DROP TABLE pod0_download_workflows;
    DROP TABLE pod0_download_environment;
    DROP TABLE pod0_recall_configuration;
";
