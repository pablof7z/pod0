use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DownloadEffectAuthorization,
    DownloadTransition, DurableExternalEffectRequest, DurableInternalCommandRequest,
    ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, RequestRejectionReason,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadControlOperation {
    Cancel,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadControlActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub legacy_replay: bool,
    pub operation: DownloadControlOperation,
    pub rejection: Option<RequestRejectionReason>,
    pub effect: Option<DownloadEffectAuthorization>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadControlMutation {
    Apply,
    RecordRejection,
    LegacyDuplicate,
}

pub type DownloadControlPlan = TransitionPlan<
    DownloadControlMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_download_control(
    input: DownloadControlActivityInput,
) -> Result<DownloadControlPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let (mutation, disposition) = if input.legacy_replay {
        (
            DownloadControlMutation::LegacyDuplicate,
            RequestDisposition::Duplicate,
        )
    } else if let Some(reason) = input.rejection {
        (
            DownloadControlMutation::RecordRejection,
            RequestDisposition::Rejected { reason },
        )
    } else {
        (DownloadControlMutation::Apply, RequestDisposition::Accepted)
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    if input.effect.is_some() && mutation != DownloadControlMutation::Apply {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let mut effects = Vec::new();
    let facts = if mutation == DownloadControlMutation::Apply {
        let committed = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let transition = |ordinal, kind| {
            base(
                ordinal,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Download(kind),
                    previous_revision: input.current_revision,
                    committed_revision: committed,
                },
            )
        };
        let mut tail = vec![
            transition(1, DownloadTransition::DesiredStateChanged),
            transition(2, DownloadTransition::AttemptStateChanged),
        ];
        if let Some(effect) = input.effect {
            if effect.request.episode_id() != input.episode_id {
                return Err(TransitionPlanError::InvalidEffectAuthorization);
            }
            let intent_id = identity.effect_intent_id(0);
            tail.push(base(
                3,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::Download,
                },
            ));
            effects.push(AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: 3,
                request: DurableExternalEffectRequest {
                    kind: ExternalEffectKind::Download,
                    subject,
                    episode_id: Some(input.episode_id),
                    not_before: effect.request.not_before,
                    deadline_at: effect.request.deadline_at,
                    execution: crate::DurableEffectExecution::Download {
                        request: effect.request,
                    },
                },
            });
        }
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
        mutation,
        facts,
        effects,
        Vec::new(),
    )
}
