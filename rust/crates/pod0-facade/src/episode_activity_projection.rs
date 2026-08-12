use std::fmt::Write as _;

use crate::Pod0Facade;
use pod0_application::{ActivityFact, CommittedActivityFact, EffectOutcome, RequestDisposition};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EpisodeActivityEntryKind {
    Request,
    DomainTransition,
    PlaybackCheckpoint,
    EffectAuthorization,
    EffectObservation,
    InternalCommand,
    Recovery,
    AuthorityCutover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EpisodeActivitySeverity {
    Info,
    Success,
    Warning,
    Failure,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeActivityDetail {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeActivityEntry {
    pub sequence: u64,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
    pub kind: EpisodeActivityEntryKind,
    pub severity: EpisodeActivitySeverity,
    pub title: String,
    pub summary: String,
    pub details: Vec<EpisodeActivityDetail>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeActivityPage {
    pub available: bool,
    pub items: Vec<EpisodeActivityEntry>,
    pub next_after_sequence: Option<u64>,
}

impl EpisodeActivityPage {
    pub(super) fn unavailable() -> Self {
        Self {
            available: false,
            items: Vec::new(),
            next_after_sequence: None,
        }
    }

    pub(super) fn from_storage(page: pod0_storage::ActivityPage) -> Self {
        Self {
            available: true,
            items: page.items.into_iter().map(project).collect(),
            next_after_sequence: page.next_after_sequence,
        }
    }
}

#[uniffi::export]
impl Pod0Facade {
    pub fn episode_activity_page(
        &self,
        episode_id: pod0_domain::EpisodeId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> EpisodeActivityPage {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return EpisodeActivityPage::unavailable();
        };
        store
            .activity_page_for_episode(episode_id, after_sequence, requested_count)
            .map(EpisodeActivityPage::from_storage)
            .unwrap_or_else(|_| EpisodeActivityPage::unavailable())
    }
}

fn project(committed: CommittedActivityFact) -> EpisodeActivityEntry {
    let (kind, severity, title, summary) = presentation(committed.draft.fact);
    let mut details = vec![
        detail("Activity", opaque(committed.draft.activity_id.into_bytes())),
        detail(
            "Transaction",
            opaque(committed.draft.transaction_id.into_bytes()),
        ),
        detail(
            "Correlation",
            opaque(committed.draft.correlation_id.into_bytes()),
        ),
        detail("Actor", format!("{:?}", committed.draft.actor)),
        detail("Origin", format!("{:?}", committed.draft.origin)),
    ];
    if let Some(cause) = committed.draft.caused_by_activity_id {
        details.push(detail("Caused by", opaque(cause.into_bytes())));
    }
    if let Some(command_id) = committed.draft.command_id {
        details.push(detail("Command", opaque(command_id.into_bytes())));
    }
    if let Some(request_id) = committed.draft.host_request_id {
        details.push(detail("Host request", opaque(request_id.into_bytes())));
    }
    fact_details(committed.draft.fact, &mut details);
    EpisodeActivityEntry {
        sequence: committed.sequence,
        committed_at: committed.committed_at,
        kind,
        severity,
        title: title.to_owned(),
        summary,
        details,
    }
}

fn presentation(
    fact: ActivityFact,
) -> (
    EpisodeActivityEntryKind,
    EpisodeActivitySeverity,
    &'static str,
    String,
) {
    match fact {
        ActivityFact::RequestDisposition { disposition } => (
            EpisodeActivityEntryKind::Request,
            disposition_severity(disposition),
            "Request decision",
            format!("{disposition:?}"),
        ),
        ActivityFact::DomainTransition { kind, .. } => (
            EpisodeActivityEntryKind::DomainTransition,
            EpisodeActivitySeverity::Success,
            "State transition",
            format!("{kind:?}"),
        ),
        ActivityFact::PlaybackCheckpoint {
            position_milliseconds,
        } => (
            EpisodeActivityEntryKind::PlaybackCheckpoint,
            EpisodeActivitySeverity::Info,
            "Playback checkpoint",
            format!("Committed at {position_milliseconds} ms"),
        ),
        ActivityFact::EffectAuthorized { kind, .. } => (
            EpisodeActivityEntryKind::EffectAuthorization,
            EpisodeActivitySeverity::Info,
            "External action authorized",
            format!("{kind:?}"),
        ),
        ActivityFact::EffectObserved { outcome, .. } => (
            EpisodeActivityEntryKind::EffectObservation,
            outcome_severity(outcome),
            "External action observed",
            format!("{outcome:?}"),
        ),
        ActivityFact::InternalCommandAuthorized { target, .. } => (
            EpisodeActivityEntryKind::InternalCommand,
            EpisodeActivitySeverity::Info,
            "Internal work authorized",
            format!("{target:?}"),
        ),
        ActivityFact::RecoveryTransition { outcome } => (
            EpisodeActivityEntryKind::Recovery,
            outcome_severity(outcome),
            "Recovery transition",
            format!("{outcome:?}"),
        ),
        ActivityFact::AuthorityCutover { domain } => (
            EpisodeActivityEntryKind::AuthorityCutover,
            EpisodeActivitySeverity::Success,
            "Authority cutover",
            format!("{domain:?}"),
        ),
    }
}

fn fact_details(fact: ActivityFact, details: &mut Vec<EpisodeActivityDetail>) {
    match fact {
        ActivityFact::DomainTransition {
            previous_revision,
            committed_revision,
            ..
        } => {
            details.push(detail(
                "Previous revision",
                previous_revision.value.to_string(),
            ));
            details.push(detail(
                "Committed revision",
                committed_revision.value.to_string(),
            ));
        }
        ActivityFact::EffectAuthorized { intent_id, .. }
        | ActivityFact::EffectObserved { intent_id, .. } => {
            details.push(detail("Effect intent", opaque(intent_id.into_bytes())));
            if let ActivityFact::EffectObserved { attempt_id, .. } = fact {
                details.push(detail("Effect attempt", opaque(attempt_id.into_bytes())));
            }
        }
        ActivityFact::InternalCommandAuthorized {
            internal_command_id,
            ..
        } => details.push(detail(
            "Internal command",
            opaque(internal_command_id.into_bytes()),
        )),
        _ => {}
    }
}

const fn disposition_severity(value: RequestDisposition) -> EpisodeActivitySeverity {
    match value {
        RequestDisposition::Accepted | RequestDisposition::AlreadyComplete => {
            EpisodeActivitySeverity::Success
        }
        RequestDisposition::Rejected { .. } => EpisodeActivitySeverity::Failure,
        RequestDisposition::Stale
        | RequestDisposition::Duplicate
        | RequestDisposition::NoSemanticChange => EpisodeActivitySeverity::Info,
    }
}

const fn outcome_severity(value: EffectOutcome) -> EpisodeActivitySeverity {
    match value {
        EffectOutcome::Succeeded => EpisodeActivitySeverity::Success,
        EffectOutcome::Failed { .. } => EpisodeActivitySeverity::Failure,
        EffectOutcome::Cancelled | EffectOutcome::Superseded => EpisodeActivitySeverity::Warning,
        EffectOutcome::OutcomeUnknown => EpisodeActivitySeverity::Warning,
    }
}

fn detail(label: &str, value: String) -> EpisodeActivityDetail {
    EpisodeActivityDetail {
        label: label.to_owned(),
        value,
    }
}

fn opaque(bytes: [u8; 16]) -> String {
    let mut output = String::with_capacity(32);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use pod0_application::{ActivityActor, ActivityFactDraft, ActivityOrigin, ActivitySubject};
    use pod0_domain::{
        ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, EpisodeId,
        StateRevision, UnixTimestampMilliseconds,
    };

    use super::*;

    #[test]
    fn projection_contains_only_typed_redacted_fact_fields() {
        let secret = "private transcript text and provider token";
        let episode_id = EpisodeId::from_parts(1, 2);
        let entry = project(CommittedActivityFact {
            sequence: 9,
            committed_at: UnixTimestampMilliseconds::new(10),
            draft: ActivityFactDraft {
                activity_id: ActivityId::from_parts(2, 1),
                transaction_id: ActivityTransactionId::from_parts(2, 2),
                correlation_id: ActivityCorrelationId::from_parts(2, 3),
                caused_by_activity_id: None,
                command_id: Some(CommandId::from_parts(2, 4)),
                host_request_id: None,
                actor: ActivityActor::User,
                origin: ActivityOrigin::UserInterface,
                subject: ActivitySubject::Episode { episode_id },
                episode_id: Some(episode_id),
                fact: ActivityFact::DomainTransition {
                    kind: pod0_application::DomainTransitionKind::Playback(
                        pod0_application::PlaybackTransition::SessionStateChanged,
                    ),
                    previous_revision: StateRevision::new(4),
                    committed_revision: StateRevision::new(5),
                },
            },
        });
        let rendered = format!("{entry:?}");
        assert!(!rendered.contains(secret));
        assert_eq!(entry.sequence, 9);
        assert_eq!(entry.kind, EpisodeActivityEntryKind::DomainTransition);
    }
}
