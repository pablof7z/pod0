use crate::settings_transition_candidate::settings_candidate;
use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, RequestRejectionReason, TransitionPlan,
    TransitionPlanError, UserArtifactTransition,
};
use crate::{
    SettingsValidationCode, SettingsValidationState, settings_values_digest,
    validate_product_settings,
};
use pod0_domain::{
    CommandId, ContentDigest, PRODUCT_SETTINGS_SCHEMA_VERSION, ProductSettings,
    ProductSettingsValues, SettingsWriterVersion, StateRevision,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsChange {
    LegacyImport {
        source_generation: u64,
        writer_id: ContentDigest,
        values: ProductSettingsValues,
    },
    Local {
        expected_revision: StateRevision,
        writer_id: ContentDigest,
        values: ProductSettingsValues,
    },
    Remote {
        schema_version: u32,
        writer_version: SettingsWriterVersion,
        values: ProductSettingsValues,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsChangeSource {
    LegacyImport,
    Local,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsConflictWinner {
    Current,
    Candidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SettingsConflictEvidence {
    pub current_version: SettingsWriterVersion,
    pub candidate_version: SettingsWriterVersion,
    pub current_digest: ContentDigest,
    pub candidate_digest: ContentDigest,
    pub winner: SettingsConflictWinner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsMutation {
    pub source: SettingsChangeSource,
    pub candidate_schema_version: u32,
    pub candidate_version: SettingsWriterVersion,
    pub source_generation: Option<u64>,
    pub commit_authority: bool,
    pub next: Option<ProductSettings>,
    pub validation: SettingsValidationState,
    pub conflict: Option<SettingsConflictEvidence>,
}

pub type SettingsTransitionPlan =
    TransitionPlan<SettingsMutation, DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_settings_transition(
    command_id: CommandId,
    current_revision: StateRevision,
    authority: bool,
    current: Option<&ProductSettings>,
    change: SettingsChange,
) -> Result<SettingsTransitionPlan, TransitionPlanError> {
    let source = source(&change);
    let source_generation = match &change {
        SettingsChange::LegacyImport {
            source_generation, ..
        } => Some(*source_generation),
        _ => None,
    };
    let revision_conflict = match &change {
        SettingsChange::Local {
            expected_revision, ..
        } => *expected_revision != current.map_or(StateRevision::INITIAL, |value| value.revision),
        _ => false,
    };
    let (schema_version, version, values) = settings_candidate(current, change)?;
    let mut validation = if schema_version == PRODUCT_SETTINGS_SCHEMA_VERSION {
        validate_product_settings(&values)
    } else {
        SettingsValidationState::Rejected {
            field: None,
            code: SettingsValidationCode::UnsupportedSchema,
        }
    };
    let (disposition, selected, conflict) = if validation != SettingsValidationState::Valid {
        (rejected_invalid(), None, None)
    } else if authority && source == SettingsChangeSource::LegacyImport {
        (RequestDisposition::AlreadyComplete, None, None)
    } else if !authority && source != SettingsChangeSource::LegacyImport {
        (
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::MissingPrerequisite,
            },
            None,
            None,
        )
    } else if revision_conflict {
        (
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict,
            },
            None,
            None,
        )
    } else {
        select(current, source, version, values, &mut validation)
    };
    let committed_revision = if selected.is_some() {
        StateRevision::new(
            current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        )
    } else {
        current_revision
    };
    let commit_authority = selected.is_some() && source == SettingsChangeSource::LegacyImport;
    let next = selected.map(|values| ProductSettings {
        schema_version: PRODUCT_SETTINGS_SCHEMA_VERSION,
        revision: committed_revision,
        writer_version: version,
        values,
    });
    build_plan(
        command_id,
        current_revision,
        committed_revision,
        disposition,
        SettingsMutation {
            source,
            candidate_schema_version: schema_version,
            candidate_version: version,
            source_generation,
            commit_authority,
            next,
            validation,
            conflict,
        },
    )
}

fn select(
    current: Option<&ProductSettings>,
    source: SettingsChangeSource,
    version: SettingsWriterVersion,
    values: ProductSettingsValues,
    validation: &mut SettingsValidationState,
) -> (
    RequestDisposition,
    Option<ProductSettingsValues>,
    Option<SettingsConflictEvidence>,
) {
    let Some(current) = current else {
        return (RequestDisposition::Accepted, Some(values), None);
    };
    if source == SettingsChangeSource::Local && values == current.values {
        return (RequestDisposition::NoSemanticChange, None, None);
    }
    if source == SettingsChangeSource::Remote
        && version == current.writer_version
        && values != current.values
    {
        *validation = SettingsValidationState::Rejected {
            field: None,
            code: SettingsValidationCode::VersionReuse,
        };
        return (rejected_invalid(), None, None);
    }
    let conflict = conflict(current, version, &values);
    if source == SettingsChangeSource::Local || version > current.writer_version {
        (RequestDisposition::Accepted, Some(values), conflict)
    } else if version == current.writer_version {
        (RequestDisposition::NoSemanticChange, None, conflict)
    } else {
        (RequestDisposition::Stale, None, conflict)
    }
}

fn conflict(
    current: &ProductSettings,
    candidate_version: SettingsWriterVersion,
    candidate: &ProductSettingsValues,
) -> Option<SettingsConflictEvidence> {
    (candidate_version.counter == current.writer_version.counter
        && candidate_version.writer_id != current.writer_version.writer_id
        && candidate != &current.values)
        .then(|| SettingsConflictEvidence {
            current_version: current.writer_version,
            candidate_version,
            current_digest: settings_values_digest(&current.values),
            candidate_digest: settings_values_digest(candidate),
            winner: if candidate_version > current.writer_version {
                SettingsConflictWinner::Candidate
            } else {
                SettingsConflictWinner::Current
            },
        })
}

fn build_plan(
    command_id: CommandId,
    current: StateRevision,
    committed: StateRevision,
    disposition: RequestDisposition,
    mutation: SettingsMutation,
) -> Result<SettingsTransitionPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(command_id);
    let transaction_id = identity.transaction_id();
    let actor = match mutation.source {
        SettingsChangeSource::LegacyImport => ActivityActor::Migration,
        SettingsChangeSource::Local => ActivityActor::User,
        SettingsChangeSource::Remote => ActivityActor::System,
    };
    let origin = match mutation.source {
        SettingsChangeSource::LegacyImport => ActivityOrigin::Migration,
        SettingsChangeSource::Local => ActivityOrigin::UserInterface,
        SettingsChangeSource::Remote => ActivityOrigin::HostObservation,
    };
    let fact = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(command_id),
        host_request_id: None,
        actor,
        origin,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let head = fact(0, ActivityFact::RequestDisposition { disposition });
    let facts = if mutation.next.is_some() {
        let mut tail = vec![fact(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::UserArtifact(UserArtifactTransition::SettingChanged),
                previous_revision: current,
                committed_revision: committed,
            },
        )];
        if mutation.commit_authority {
            tail.push(fact(
                2,
                ActivityFact::AuthorityCutover {
                    domain: crate::ActivityDomain::UserArtifact,
                },
            ));
        }
        NonEmptyActivityFacts::from_head_and_tail(head, tail)
    } else {
        NonEmptyActivityFacts::new(head)
    };
    TransitionPlan::new(
        transaction_id,
        current,
        mutation,
        facts,
        Vec::new(),
        Vec::new(),
    )
}

fn source(change: &SettingsChange) -> SettingsChangeSource {
    match change {
        SettingsChange::LegacyImport { .. } => SettingsChangeSource::LegacyImport,
        SettingsChange::Local { .. } => SettingsChangeSource::Local,
        SettingsChange::Remote { .. } => SettingsChangeSource::Remote,
    }
}

const fn rejected_invalid() -> RequestDisposition {
    RequestDisposition::Rejected {
        reason: RequestRejectionReason::Invalid,
    }
}
