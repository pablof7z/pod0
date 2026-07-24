import Foundation

/// Process-local cutover flags used to keep verified Rust projections out of
/// native durable storage while preserving pre-cutover migration evidence.
struct SharedArtifactAuthority {
    var listening = false
    var notes = false
    var clips = false
    var scheduledAgents = false
    var memories = false
    var legacyAgentActivityRetired = false
}
