use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DownloadAdmissionPlan,
    DownloadEffectAuthorization, DownloadTransition, DurableExternalEffectRequest,
    ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRecoveryActivityInput {
    pub identity_command_id: CommandId,
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub transition: DownloadTransition,
    pub effect: Option<DownloadEffectAuthorization>,
}

pub fn plan_download_recovery(
    input: DownloadRecoveryActivityInput,
) -> Result<DownloadAdmissionPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.identity_command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::Recovery,
        origin: ActivityOrigin::Recovery,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let mut tail = vec![base(
        1,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Download(input.transition),
            previous_revision: input.current_revision,
            committed_revision,
        },
    )];
    if input
        .effect
        .as_ref()
        .is_some_and(|effect| effect.request.episode_id() != input.episode_id)
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let effects = input.effect.map_or_else(Vec::new, |effect| {
        let intent_id = identity.effect_intent_id(0);
        tail.push(base(
            2,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::Download,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
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
        }]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            tail,
        ),
        effects,
        Vec::new(),
    )
}
