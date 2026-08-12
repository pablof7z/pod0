use pod0_domain::{
    ActivityId, ActivityTransactionId, EffectIntentId, InternalCommandId, StateRevision,
};

use crate::{
    ActivityDomain, ActivityFact, ActivityFactDraft, ExternalEffectKind, ExternalEffectRequest,
    InternalCommandRequest, RequestDisposition,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonEmptyActivityFacts {
    head: ActivityFactDraft,
    tail: Vec<ActivityFactDraft>,
}

impl NonEmptyActivityFacts {
    #[must_use]
    pub const fn new(head: ActivityFactDraft) -> Self {
        Self {
            head,
            tail: Vec::new(),
        }
    }

    #[must_use]
    pub fn from_head_and_tail(head: ActivityFactDraft, tail: Vec<ActivityFactDraft>) -> Self {
        Self { head, tail }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&ActivityFactDraft> {
        if index == 0 {
            Some(&self.head)
        } else {
            self.tail.get(index - 1)
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ActivityFactDraft> {
        std::iter::once(&self.head).chain(self.tail.iter())
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<ActivityFactDraft> {
        let mut values = Vec::with_capacity(self.len());
        values.push(self.head);
        values.extend(self.tail);
        values
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedExternalEffect<E> {
    pub intent_id: EffectIntentId,
    pub authorizing_fact_index: usize,
    pub request: E,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedInternalCommand<C> {
    pub internal_command_id: InternalCommandId,
    pub authorizing_fact_index: usize,
    pub command: C,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionPlan<M, E, C> {
    transaction_id: ActivityTransactionId,
    expected_revision: StateRevision,
    mutation: M,
    facts: NonEmptyActivityFacts,
    external_effects: Vec<AuthorizedExternalEffect<E>>,
    internal_commands: Vec<AuthorizedInternalCommand<C>>,
    disposition: RequestDisposition,
}

pub type TransitionPlanParts<M, E, C> = (
    ActivityTransactionId,
    StateRevision,
    M,
    NonEmptyActivityFacts,
    Vec<AuthorizedExternalEffect<E>>,
    Vec<AuthorizedInternalCommand<C>>,
    RequestDisposition,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionPlanError {
    MissingAuthorizingFact {
        index: usize,
    },
    ExternalEffectAuthorizationMismatch {
        index: usize,
        expected: EffectIntentId,
        actual_activity_id: ActivityId,
    },
    ExternalEffectKindMismatch {
        index: usize,
        expected: ExternalEffectKind,
        actual: ExternalEffectKind,
    },
    ExternalEffectSubjectMismatch {
        index: usize,
    },
    InternalCommandAuthorizationMismatch {
        index: usize,
        expected: InternalCommandId,
        actual_activity_id: ActivityId,
    },
    InternalCommandTargetMismatch {
        index: usize,
        expected: ActivityDomain,
        actual: ActivityDomain,
    },
    InternalCommandSubjectMismatch {
        index: usize,
    },
    TransactionIdentityMismatch {
        index: usize,
        actual_activity_id: ActivityId,
    },
    CorrelationIdentityMismatch {
        index: usize,
        actual_activity_id: ActivityId,
    },
    MissingRequestDisposition,
    MultipleRequestDispositions,
    RevisionExhausted,
    DispositionRequiresTransition,
    TooManyInternalCommands,
}

impl<M, E: ExternalEffectRequest, C: InternalCommandRequest> TransitionPlan<M, E, C> {
    pub fn new(
        transaction_id: ActivityTransactionId,
        expected_revision: StateRevision,
        mutation: M,
        facts: NonEmptyActivityFacts,
        external_effects: Vec<AuthorizedExternalEffect<E>>,
        internal_commands: Vec<AuthorizedInternalCommand<C>>,
    ) -> Result<Self, TransitionPlanError> {
        let disposition = {
            let mut dispositions = facts.iter().filter_map(|draft| match draft.fact {
                ActivityFact::RequestDisposition { disposition } => Some(disposition),
                _ => None,
            });
            let disposition = dispositions
                .next()
                .ok_or(TransitionPlanError::MissingRequestDisposition)?;
            if dispositions.next().is_some() {
                return Err(TransitionPlanError::MultipleRequestDispositions);
            }
            disposition
        };
        let correlation_id = facts.get(0).expect("non-empty facts").correlation_id;
        for (index, draft) in facts.iter().enumerate() {
            if draft.transaction_id != transaction_id {
                return Err(TransitionPlanError::TransactionIdentityMismatch {
                    index,
                    actual_activity_id: draft.activity_id,
                });
            }
            if draft.correlation_id != correlation_id {
                return Err(TransitionPlanError::CorrelationIdentityMismatch {
                    index,
                    actual_activity_id: draft.activity_id,
                });
            }
        }
        for effect in &external_effects {
            let Some(draft) = facts.get(effect.authorizing_fact_index) else {
                return Err(TransitionPlanError::MissingAuthorizingFact {
                    index: effect.authorizing_fact_index,
                });
            };
            let ActivityFact::EffectAuthorized { intent_id, kind } = draft.fact else {
                return Err(TransitionPlanError::ExternalEffectAuthorizationMismatch {
                    index: effect.authorizing_fact_index,
                    expected: effect.intent_id,
                    actual_activity_id: draft.activity_id,
                });
            };
            if intent_id != effect.intent_id {
                return Err(TransitionPlanError::ExternalEffectAuthorizationMismatch {
                    index: effect.authorizing_fact_index,
                    expected: effect.intent_id,
                    actual_activity_id: draft.activity_id,
                });
            }
            let actual = effect.request.effect_kind();
            if kind != actual {
                return Err(TransitionPlanError::ExternalEffectKindMismatch {
                    index: effect.authorizing_fact_index,
                    expected: kind,
                    actual,
                });
            }
            if draft.subject != effect.request.subject()
                || draft.episode_id != effect.request.episode_id()
            {
                return Err(TransitionPlanError::ExternalEffectSubjectMismatch {
                    index: effect.authorizing_fact_index,
                });
            }
        }
        for command in &internal_commands {
            let Some(draft) = facts.get(command.authorizing_fact_index) else {
                return Err(TransitionPlanError::MissingAuthorizingFact {
                    index: command.authorizing_fact_index,
                });
            };
            let ActivityFact::InternalCommandAuthorized {
                internal_command_id,
                target,
            } = draft.fact
            else {
                return Err(TransitionPlanError::InternalCommandAuthorizationMismatch {
                    index: command.authorizing_fact_index,
                    expected: command.internal_command_id,
                    actual_activity_id: draft.activity_id,
                });
            };
            if internal_command_id != command.internal_command_id {
                return Err(TransitionPlanError::InternalCommandAuthorizationMismatch {
                    index: command.authorizing_fact_index,
                    expected: command.internal_command_id,
                    actual_activity_id: draft.activity_id,
                });
            }
            let actual = command.command.target_domain();
            if target != actual {
                return Err(TransitionPlanError::InternalCommandTargetMismatch {
                    index: command.authorizing_fact_index,
                    expected: target,
                    actual,
                });
            }
            if draft.subject != command.command.subject()
                || draft.episode_id != command.command.episode_id()
            {
                return Err(TransitionPlanError::InternalCommandSubjectMismatch {
                    index: command.authorizing_fact_index,
                });
            }
        }
        Ok(Self {
            transaction_id,
            expected_revision,
            mutation,
            facts,
            external_effects,
            internal_commands,
            disposition,
        })
    }

    #[must_use]
    pub const fn disposition(&self) -> RequestDisposition {
        self.disposition
    }

    #[must_use]
    pub fn into_parts(self) -> TransitionPlanParts<M, E, C> {
        (
            self.transaction_id,
            self.expected_revision,
            self.mutation,
            self.facts,
            self.external_effects,
            self.internal_commands,
            self.disposition,
        )
    }
}
include!("transition_plan_mapping.rs");
