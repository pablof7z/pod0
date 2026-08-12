impl<M, E, C> TransitionPlan<M, E, C> {
    /// Replaces only the storage mutation payload while preserving the already
    /// validated facts and authorization graph.
    #[must_use]
    pub fn map_mutation<N>(self, map: impl FnOnce(M) -> N) -> TransitionPlan<N, E, C> {
        TransitionPlan {
            transaction_id: self.transaction_id,
            expected_revision: self.expected_revision,
            mutation: map(self.mutation),
            facts: self.facts,
            external_effects: self.external_effects,
            internal_commands: self.internal_commands,
            disposition: self.disposition,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionDisposition {
    pub disposition: RequestDisposition,
    pub fact: ActivityFactDraft,
}

impl TransitionDisposition {
    #[must_use]
    pub fn new(fact: ActivityFactDraft, disposition: RequestDisposition) -> Option<Self> {
        matches!(
            fact.fact,
            ActivityFact::RequestDisposition {
                disposition: fact_disposition
            } if fact_disposition == disposition
        )
        .then_some(Self { disposition, fact })
    }
}
