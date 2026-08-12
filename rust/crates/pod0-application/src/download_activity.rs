use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EpisodeId, InternalCommandId, StateRevision,
};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedInternalCommand, DomainTransitionKind, DownloadIntentOrigin,
    DownloadTransition, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadAdmissionActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub legacy_replay: bool,
    pub state_changes: bool,
    pub admitted: bool,
    pub origin: DownloadIntentOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadDispositionActivityInput {
    pub command_id: CommandId,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub legacy_replay: bool,
    pub origin: DownloadIntentOrigin,
    pub internal_commands: Vec<DurableInternalCommandRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadInternalAdmissionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub state_changes: bool,
    pub admitted: bool,
    pub disposition: RequestDisposition,
}

pub type DownloadAdmissionPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_download_admission(
    input: DownloadAdmissionActivityInput,
) -> Result<DownloadAdmissionPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let (actor, origin) = activity_origin(input.origin);
    let disposition = if input.legacy_replay {
        RequestDisposition::Duplicate
    } else if !input.state_changes {
        RequestDisposition::NoSemanticChange
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor,
        origin,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let facts = if disposition == RequestDisposition::Accepted {
        let committed = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let tail = vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Download(if input.admitted {
                    DownloadTransition::AttemptStateChanged
                } else {
                    DownloadTransition::DesiredStateChanged
                }),
                previous_revision: input.current_revision,
                committed_revision: committed,
            },
        )];
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            tail,
        )
    } else {
        NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition }))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        Vec::new(),
        Vec::new(),
    )
}

pub fn plan_download_noop(
    input: DownloadDispositionActivityInput,
) -> Result<DownloadAdmissionPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let (actor, origin) = activity_origin(input.origin);
    if input.internal_commands.len() > 200 {
        return Err(TransitionPlanError::TooManyInternalCommands);
    }
    let disposition = if input.legacy_replay {
        RequestDisposition::Duplicate
    } else if !input.internal_commands.is_empty() {
        RequestDisposition::Accepted
    } else {
        RequestDisposition::NoSemanticChange
    };
    let transaction_id = identity.transaction_id();
    let head = ActivityFactDraft {
        activity_id: identity.fact_id(0),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor,
        origin,
        subject: input.subject,
        episode_id: input.episode_id,
        fact: ActivityFact::RequestDisposition { disposition },
    };
    let mut tail = Vec::new();
    let mut commands = Vec::new();
    if !input.legacy_replay {
        for (index, command) in input.internal_commands.into_iter().enumerate() {
            let ordinal =
                u8::try_from(index).map_err(|_| TransitionPlanError::TooManyInternalCommands)?;
            let internal_command_id = identity.internal_command_id(ordinal);
            tail.push(ActivityFactDraft {
                activity_id: identity.fact_id(ordinal.saturating_add(1)),
                transaction_id,
                correlation_id: identity.correlation_id(),
                caused_by_activity_id: None,
                command_id: Some(input.command_id),
                host_request_id: None,
                actor,
                origin,
                subject: command.subject,
                episode_id: command.episode_id,
                fact: ActivityFact::InternalCommandAuthorized {
                    internal_command_id,
                    target: ActivityDomain::Download,
                },
            });
            commands.push(AuthorizedInternalCommand {
                internal_command_id,
                authorizing_fact_index: index + 1,
                command,
            });
        }
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(head, tail),
        Vec::new(),
        commands,
    )
}

pub fn plan_download_internal_admission(
    input: DownloadInternalAdmissionActivityInput,
) -> Result<DownloadAdmissionPlan, TransitionPlanError> {
    let identity = crate::InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    if input.state_changes != (input.disposition == RequestDisposition::Accepted) {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let disposition = input.disposition;
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let facts = if input.state_changes {
        let committed = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Download(if input.admitted {
                        DownloadTransition::AttemptStateChanged
                    } else {
                        DownloadTransition::DesiredStateChanged
                    }),
                    previous_revision: input.current_revision,
                    committed_revision: committed,
                },
            )],
        )
    } else {
        NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition }))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        Vec::new(),
        Vec::new(),
    )
}

const fn activity_origin(origin: DownloadIntentOrigin) -> (ActivityActor, ActivityOrigin) {
    match origin {
        DownloadIntentOrigin::User => (ActivityActor::User, ActivityOrigin::UserInterface),
        DownloadIntentOrigin::Playback => (ActivityActor::System, ActivityOrigin::Playback),
        DownloadIntentOrigin::Automatic => (ActivityActor::System, ActivityOrigin::AutomaticPolicy),
        DownloadIntentOrigin::Unsupported { .. } => {
            (ActivityActor::System, ActivityOrigin::AutomaticPolicy)
        }
    }
}
