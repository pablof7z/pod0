impl TransitionCommit {
    /// Plans and commits under the same immediate transaction. Command owners
    /// whose plan depends on current state must use this boundary so no writer
    /// can invalidate validation between admission and mutation.
    pub(super) fn commit_planned_with<M>(
        &self,
        ingress: TransitionIngress,
        committed_at: UnixTimestampMilliseconds,
        plan: impl FnOnce(&Transaction<'_>) -> Result<TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>, StorageError>,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_planned_with_hooks_and_fault(|_| Ok(ingress), committed_at, plan, |_| Ok(()), mutate, |_| Ok(()), |_| Ok(()))
    }

    pub(super) fn commit_planned_with_transaction_hooks<M>(
        &self,
        ingress: TransitionIngress,
        committed_at: UnixTimestampMilliseconds,
        plan: impl FnOnce(&Transaction<'_>) -> Result<TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>, StorageError>,
        before_mutation: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
        after_activity: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_planned_with_hooks_and_fault(
            |_| Ok(ingress),
            committed_at,
            plan,
            before_mutation,
            mutate,
            after_activity,
            |_| Ok(()),
        )
    }

    /// Resolves state-derived ingress identity under the writer lock before
    /// replay lookup and full transition planning.
    pub(super) fn commit_resolved_ingress_with<M>(
        &self,
        committed_at: UnixTimestampMilliseconds,
        ingress: impl FnOnce(&Transaction<'_>) -> Result<TransitionIngress, StorageError>,
        plan: impl FnOnce(&Transaction<'_>) -> Result<TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>, StorageError>,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_planned_with_hooks_and_fault(
            ingress,
            committed_at,
            plan,
            |_| Ok(()),
            mutate,
            |_| Ok(()),
            |_| Ok(()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_planned_with_hooks_and_fault<M>(
        &self,
        ingress: impl FnOnce(&Transaction<'_>) -> Result<TransitionIngress, StorageError>,
        committed_at: UnixTimestampMilliseconds,
        plan: impl FnOnce(&Transaction<'_>) -> Result<TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>, StorageError>,
        before_mutation: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
        after_activity: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
        mut fault: impl FnMut(CommitFaultPoint) -> Result<(), StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        let mut connection = open_connection(&self.path, false)?;
        validate_current_database_identity(&connection, user_version(&connection)?)?;
        configure(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin transition commit", error))?;
        let ingress = ingress(&transaction)?;
        if let Some(receipt) = prior_receipt(&transaction, ingress)? {
            return Ok(receipt);
        }
        let plan = plan(&transaction)?;
        let (transaction_id, expected_revision, mutation, facts, effects, internal_commands, disposition) = plan.into_parts();
        let consumed_internal_command = if ingress.kind == TransitionIngressKind::InternalCommand {
            Some(require_internal_command(&transaction, ingress.id, &facts)?)
        } else {
            None
        };
        before_mutation(&transaction)?;
        fault(CommitFaultPoint::BeforeMutation)?;
        let committed_revision = mutate(&transaction, expected_revision, mutation)?;
        fault(CommitFaultPoint::AfterMutation)?;
        let committed = append_activity_facts(&JournalAppendAuthority(()), &transaction, &facts, committed_at)?;
        fault(CommitFaultPoint::AfterFacts)?;
        for effect in effects {
            let authorizing = facts.get(effect.authorizing_fact_index).expect("validated effect authorization");
            append_effect_intent(&transaction, authorizing, effect.intent_id.into_bytes(), effect.request, committed_at)?;
        }
        fault(CommitFaultPoint::AfterEffectIntents)?;
        for command in internal_commands {
            let authorizing = facts.get(command.authorizing_fact_index).expect("validated internal command authorization");
            append_internal_command(&transaction, authorizing, command.internal_command_id.into_bytes(), command.command, committed_at)?;
        }
        if let Some(command_id) = consumed_internal_command {
            let changed = transaction.execute(
                "UPDATE pod0_internal_command_intents SET state_code=2 WHERE internal_command_id=?1 AND state_code=1",
                [command_id.as_slice()],
            ).map_err(|error| StorageError::sqlite("consume internal command", error))?;
            if changed != 1 { return Err(StorageError::ActivityCommandConflict); }
        }
        after_activity(&transaction)?;
        fault(CommitFaultPoint::AfterInternalCommands)?;
        let first_sequence = committed.first().expect("non-empty facts").sequence;
        let last_sequence = committed.last().expect("non-empty facts").sequence;
        append_receipt(&transaction, ingress, transaction_id, disposition, first_sequence, last_sequence, committed_revision, committed_at)?;
        fault(CommitFaultPoint::AfterReceipt)?;
        transaction.commit().map_err(|error| StorageError::sqlite("commit transition", error))?;
        Ok(CommitReceipt { transaction_id, disposition, first_sequence, last_sequence, committed_revision, replayed: false })
    }
}
