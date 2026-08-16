impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_feed_discovery_recovery(
        &self,
        phase: &'static [u8],
        occurrence_id: FeedDiscoveryOccurrenceId,
        subject: ActivitySubject,
        episode_id: Option<EpisodeId>,
        observed_at_ms: i64,
        identity_variant: impl Fn(&Transaction<'_>) -> Result<bool, StorageError>,
        inspect: impl FnOnce(&Transaction<'_>) -> Result<bool, StorageError>,
        apply: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
    ) -> Result<bool, StorageError> {
        let identity_variant = &identity_variant;
        let receipt = TransitionCommit::open(self.path())?.commit_resolved_ingress_with(
            UnixTimestampMilliseconds::new(observed_at_ms),
            |transaction| {
                let current = current_library_revision(transaction)?;
                let (recovery_id, fingerprint) =
                    crate::feed_discovery_workflow_store::feed_discovery_recovery_identity(
                        phase,
                        occurrence_id,
                        episode_id,
                        current,
                        identity_variant(transaction)?,
                    );
                Ok(TransitionIngress {
                    kind: TransitionIngressKind::Recovery,
                    id: recovery_id.into_bytes(),
                    fingerprint,
                })
            },
            |transaction| {
                let current = current_library_revision(transaction)?;
                let (recovery_id, _) =
                    crate::feed_discovery_workflow_store::feed_discovery_recovery_identity(
                        phase,
                        occurrence_id,
                        episode_id,
                        current,
                        identity_variant(transaction)?,
                    );
                let state_changes = inspect(transaction)?;
                pod0_application::plan_feed_discovery_recovery(
                    pod0_application::FeedDiscoveryRecoveryInput {
                        recovery_id,
                        subject,
                        episode_id,
                        current_revision: current,
                        state_changes,
                    },
                )
                .map(|plan| plan.map_mutation(|()| state_changes))
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, state_changes| {
                require_revision(transaction, expected)?;
                if state_changes {
                    apply(transaction)?;
                    crate::library_store::advance_playback_revision(transaction)
                } else {
                    Ok(expected)
                }
            },
        )?;
        Ok(matches!(
            receipt.disposition,
            pod0_application::RequestDisposition::Accepted
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_library_activity<M>(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        subject: ActivitySubject,
        episode_id: Option<EpisodeId>,
        transition: LibraryFeedTransition,
        observed_at_ms: i64,
        prepare: impl FnOnce(&Transaction<'_>) -> Result<(bool, M), StorageError>,
        apply: impl FnOnce(&Transaction<'_>, M) -> Result<(), StorageError>,
        after_revision: impl FnOnce(&Transaction<'_>, StateRevision) -> Result<(), StorageError>,
    ) -> Result<StateRevision, StorageError> {
        let receipt = TransitionCommit::open(self.path())?
            .commit_planned_with(
                TransitionIngress {
                    kind: TransitionIngressKind::ApplicationCommand,
                    id: command_id.into_bytes(),
                    fingerprint: fingerprint_digest(fingerprint)?,
                },
                UnixTimestampMilliseconds::new(observed_at_ms),
                |transaction| {
                    let current = current_library_revision(transaction)?;
                    let legacy = legacy_library_revision(transaction, command_id, fingerprint)?;
                    if let Some(committed_revision) = legacy {
                        return plan_library_command(LibraryCommandActivityInput {
                            command_id,
                            subject,
                            episode_id,
                            current_revision: current,
                            legacy_command_revision: Some(committed_revision),
                            transition,
                            semantic_change: false,
                        })
                        .map(|plan| {
                            plan.map_mutation(|_| LibraryStorageMutation::Duplicate {
                                committed_revision,
                            })
                        })
                        .map_err(|_| StorageError::InvalidActivity);
                    }
                    let (semantic_change, payload) = prepare(transaction)?;
                    plan_library_command(LibraryCommandActivityInput {
                        command_id,
                        subject,
                        episode_id,
                        current_revision: current,
                        legacy_command_revision: None,
                        transition,
                        semantic_change,
                    })
                    .map(|plan| {
                        plan.map_mutation(|mutation| match mutation {
                            LibraryCommandMutation::Apply => LibraryStorageMutation::Apply(payload),
                            LibraryCommandMutation::RecordNoChange => {
                                LibraryStorageMutation::RecordNoChange
                            }
                            LibraryCommandMutation::Duplicate { committed_revision } => {
                                LibraryStorageMutation::Duplicate { committed_revision }
                            }
                        })
                    })
                    .map_err(|_| StorageError::InvalidActivity)
                },
                |transaction, expected, mutation| match mutation {
                    LibraryStorageMutation::Apply(payload) => {
                        require_revision(transaction, expected)?;
                        apply(transaction, payload)?;
                        let revision =
                            finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
                        after_revision(transaction, revision)?;
                        Ok(revision)
                    }
                    LibraryStorageMutation::RecordNoChange => {
                        require_revision(transaction, expected)?;
                        record_no_change(
                            transaction,
                            command_id,
                            fingerprint,
                            expected,
                            observed_at_ms,
                        )?;
                        Ok(expected)
                    }
                    LibraryStorageMutation::Duplicate { committed_revision } => {
                        Ok(committed_revision)
                    }
                },
            )
            .map_err(|error| match error {
                StorageError::ActivityCommandConflict => StorageError::CommandConflict,
                other => other,
            })?;
        Ok(receipt.committed_revision)
    }
}

enum LibraryStorageMutation<M> {
    Apply(M),
    RecordNoChange,
    Duplicate { committed_revision: StateRevision },
}

fn current_library_revision(transaction: &Transaction<'_>) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read library activity revision", error))?;
    revision_value(value)
}

fn legacy_library_revision(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
) -> Result<Option<StateRevision>, StorageError> {
    let legacy = transaction
        .query_row(
            "SELECT command_fingerprint,applied_revision FROM pod0_library_commands \
             WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read library activity receipt", error))?;
    match legacy {
        Some((stored, revision)) if stored == fingerprint => Ok(Some(revision_value(revision)?)),
        Some(_) => Err(StorageError::CommandConflict),
        None => Ok(None),
    }
}
