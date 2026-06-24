//! Knowledge / RAG ActionModule — routes all `"podcast.knowledge.*"` dispatches.
//!
//! Stub implementation for feature #38. The production
//! `podcast-knowledge` crate (M6.A) owns the real chunk store + hybrid
//! ranker; this module gives the iOS shell a stable wire contract while
//! that pipeline is being wired up.
//!
//! Swift encodes every knowledge action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can run the search
//! against the `PodcastStore` and stage results into the snapshot slot.
//!
//! ## Wire shape
//!
//! ```text
//! podcast.knowledge.search        — {"op":"search","query":"…"}
//! podcast.knowledge.clear_results — {"op":"clear_results"}
//! podcast.knowledge.index_episode — {"op":"index_episode","episode_id":"…"}
//! ```

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// `podcast.knowledge.search` — issue a semantic search over the
/// library. M6.B replaces the stub with a hybrid KNN + BM25 ranker.
pub const ACTION_KNOWLEDGE_SEARCH: &str = "podcast.knowledge.search";

/// `podcast.knowledge.clear_results` — drop the staged search results
/// from the snapshot. The iOS shell dispatches this when the user
/// clears the query so the next snapshot tick doesn't carry stale rows.
pub const ACTION_KNOWLEDGE_CLEAR_RESULTS: &str = "podcast.knowledge.clear_results";

/// `podcast.knowledge.index_episode` — mark an episode as ingested into
/// the knowledge store. Stubbed: real ingestion lands in M6.B with the
/// transcript pipeline. Returns `{"ok":true,"status":"indexed"}` so the
/// caller can drive a UI affordance ("indexed ✓") today.
pub const ACTION_KNOWLEDGE_INDEX_EPISODE: &str = "podcast.knowledge.index_episode";

/// Wire enum for all `"podcast.knowledge"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `search` → `{"op":"search","query":"…"}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum KnowledgeAction {
    /// Run a search against the staged knowledge index. Stub
    /// implementation: case-insensitive substring match over episode
    /// title + description, top-10 by "how early did the match land".
    Search { query: String },
    /// Clear the staged search-result slot. Idempotent.
    ClearResults,
    /// Mark `episode_id` as indexed (no-op until M6.B wires real ingest).
    IndexEpisode { episode_id: String },
}

/// Action module for the `"podcast.knowledge"` namespace.
///
/// `execute` serializes the typed `KnowledgeAction` back to JSON and
/// hands it to the actor as `ActorCommand::DispatchHostOp`. The
/// installed `PodcastHostOpHandler` deserializes it, runs the matching
/// op against the `PodcastStore` (search) or the staged results slot
/// (clear_results), and returns a `{"ok":true}` envelope. All policy
/// lives in the handler; the action module is pure routing.
pub struct KnowledgeActionModule;

impl ActionModule for KnowledgeActionModule {
    const NAMESPACE: &'static str = "podcast.knowledge";

    type Action = KnowledgeAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE, &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "knowledge_module_tests.rs"]
mod tests;
