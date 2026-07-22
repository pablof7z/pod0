use pod0_application::{ScheduledAgentOccurrenceState, ScheduledTaskDefinition};
use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

pub const MAX_LEGACY_SCHEDULED_AGENT_OCCURRENCES: usize = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduledAgentCutoverState {
    NotStarted,
    Staged { source_generation: u64 },
    Verified { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl ScheduledAgentCutoverState {
    pub const fn source_generation(self) -> Option<u64> {
        match self {
            Self::NotStarted => None,
            Self::Staged { source_generation }
            | Self::Verified { source_generation }
            | Self::Authoritative { source_generation } => Some(source_generation),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyScheduledAgentTask {
    pub definition: ScheduledTaskDefinition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyScheduledAgentOccurrence {
    pub scheduled_for: UnixTimestampMilliseconds,
    pub created_at: UnixTimestampMilliseconds,
    pub state: ScheduledAgentOccurrenceState,
    pub output_excerpt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyScheduledAgentCutoverInput {
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub tasks: Vec<LegacyScheduledAgentTask>,
    pub occurrences: Vec<LegacyScheduledAgentOccurrence>,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyScheduledAgentCutoverReport {
    pub state: ScheduledAgentCutoverState,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub task_count: u32,
    pub occurrence_count: u32,
}

pub fn scheduled_agent_cutover_source_fingerprint(
    input: &LegacyScheduledAgentCutoverInput,
) -> ContentDigest {
    let mut hash = StableCutoverHash::new();
    hash.bytes(&input.backup_digest.into_bytes());
    hash.u64(input.backup_byte_count);
    let mut tasks: Vec<_> = input.tasks.iter().collect();
    tasks.sort_by_key(|task| task.definition.task_id.into_bytes());
    hash.u64(tasks.len() as u64);
    for task in tasks {
        let definition = &task.definition;
        hash.bytes(&definition.task_id.into_bytes());
        hash.text(&definition.label);
        hash.text(&definition.prompt);
        hash.bytes(&definition.prompt_revision.into_bytes());
        hash.text(&definition.model_reference);
        hash.u64(definition.interval_milliseconds);
        hash.i64(definition.created_at.value());
        hash.optional_i64(definition.last_run_at.map(|value| value.value()));
        hash.i64(definition.next_run_at.value());
        hash.u64(definition.revision.value);
    }
    let mut occurrences: Vec<_> = input.occurrences.iter().collect();
    occurrences.sort_by_key(|occurrence| occurrence.state.occurrence_id.into_bytes());
    hash.u64(occurrences.len() as u64);
    for occurrence in occurrences {
        hash_occurrence(&mut hash, occurrence);
    }
    ContentDigest::from_bytes(hash.finish())
}

pub fn scheduled_agent_cutover_source_generation(fingerprint: ContentDigest) -> u64 {
    let bytes = fingerprint.into_bytes();
    u64::from_be_bytes(bytes[..8].try_into().expect("digest prefix")) & i64::MAX as u64 | 1
}

fn hash_occurrence(hash: &mut StableCutoverHash, value: &LegacyScheduledAgentOccurrence) {
    let state = &value.state;
    hash.bytes(&state.task_id.into_bytes());
    hash.bytes(&state.occurrence_id.into_bytes());
    hash.i64(value.scheduled_for.value());
    hash.i64(value.created_at.value());
    hash.text(&state.prompt);
    hash.bytes(&state.prompt_revision.into_bytes());
    hash.text(&state.model_reference);
    let stage =
        crate::scheduled_agent_store_codec::stage_wire(state.stage).unwrap_or("unsupported");
    hash.text(stage);
    hash.u64(state.revision.value);
    hash.u64(u64::from(state.attempt));
    hash.optional_bytes(state.attempt_id.map(|id| id.into_bytes()));
    hash.optional_bytes(state.request_id.map(|id| id.into_bytes()));
    hash.optional_text(state.provider_operation_id.as_deref());
    hash.optional_i64(state.not_before.map(|time| time.value()));
    hash.optional_bytes(state.artifact_id.map(|id| id.into_bytes()));
    hash.optional_bytes(state.output_digest.map(|digest| digest.into_bytes()));
    if let Some(failure) = &state.failure {
        hash.bytes(&[1]);
        let (name, wire) = crate::scheduled_agent_store_codec::failure_wire(failure.code);
        hash.text(name);
        hash.optional_i64(wire);
        hash.optional_text(failure.safe_detail.as_deref());
        hash.bytes(&[u8::from(failure.retryable)]);
    } else {
        hash.bytes(&[0]);
    }
    hash.i64(state.updated_at.value());
    hash.optional_text(value.output_excerpt.as_deref());
}

struct StableCutoverHash(Sha256);

impl StableCutoverHash {
    fn new() -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(b"pod0-legacy-scheduled-agent-cutover-v1");
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_be_bytes());
    }

    fn optional_i64(&mut self, value: Option<i64>) {
        match value {
            Some(value) => {
                self.bytes(&[1]);
                self.i64(value);
            }
            None => self.bytes(&[0]),
        }
    }

    fn optional_bytes<const N: usize>(&mut self, value: Option<[u8; N]>) {
        match value {
            Some(value) => {
                self.bytes(&[1]);
                self.bytes(&value);
            }
            None => self.bytes(&[0]),
        }
    }

    fn optional_text(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.bytes(&[1]);
                self.text(value);
            }
            None => self.bytes(&[0]),
        }
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}
