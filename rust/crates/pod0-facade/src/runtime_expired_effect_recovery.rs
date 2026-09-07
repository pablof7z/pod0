use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reconcile_expired_effects(&mut self) -> bool {
        let Some(store) = &self.store else {
            return false;
        };
        let now = self.now();
        let mut changed = store
            .prepare_expired_agent_model_recovery(now)
            .unwrap_or(false);
        changed |= store
            .prepare_expired_agent_capability_recovery(now)
            .unwrap_or(false);
        changed |= store
            .recover_transcript_workflows(now.value, u16::MAX)
            .map(|report| !report.ambiguous_requests.is_empty())
            .unwrap_or(false);
        changed |= store
            .recover_model_chapter_workflows(u16::MAX, now.value)
            .map(|report| !report.ambiguous_requests.is_empty())
            .unwrap_or(false);
        changed
    }
}
