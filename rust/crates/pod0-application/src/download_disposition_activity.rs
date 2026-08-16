use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedInternalCommand, DownloadAdmissionPlan, DownloadIntentOrigin,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

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
