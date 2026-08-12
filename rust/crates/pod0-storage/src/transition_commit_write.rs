fn require_internal_command(
    transaction: &Transaction<'_>,
    command_id: [u8; 16],
    facts: &NonEmptyActivityFacts,
) -> Result<[u8; 16], StorageError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = transaction
        .query_row(
            "SELECT authorizing_activity_id,correlation_id FROM pod0_internal_command_intents \
             WHERE internal_command_id=?1 AND state_code=1",
            [command_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read internal command intent", error))?;
    let Some((cause, correlation)) = row else {
        return Err(StorageError::ActivityCommandConflict);
    };
    let cause = id_bytes(&cause)?;
    let correlation = id_bytes(&correlation)?;
    let linked = facts.iter().any(|fact| {
        fact.origin == pod0_application::ActivityOrigin::InternalCommand
            && fact.caused_by_activity_id.map(|value| value.into_bytes()) == Some(cause)
            && fact.correlation_id.into_bytes() == correlation
    });
    if !linked {
        return Err(StorageError::InvalidActivity);
    }
    Ok(command_id)
}

fn prior_receipt(
    transaction: &Transaction<'_>,
    ingress: TransitionIngress,
) -> Result<Option<CommitReceipt>, StorageError> {
    let row = transaction
        .query_row(
            "SELECT fingerprint,transaction_id,disposition_code,first_sequence,last_sequence,\
             committed_revision,result_json FROM pod0_transition_receipts \
             WHERE ingress_code=?1 AND ingress_id=?2",
            params![ingress.kind.code(), ingress.id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, u8>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transition receipt", error))?;
    let Some((fingerprint, transaction_id, disposition, first, last, revision, result)) = row
    else {
        return Ok(None);
    };
    if fingerprint.as_slice() != ingress.fingerprint.into_bytes() {
        return Err(StorageError::ActivityCommandConflict);
    }
    Ok(Some(CommitReceipt {
        transaction_id: ActivityTransactionId::from_bytes(id_bytes(&transaction_id)?),
        disposition: decode_disposition(disposition, &result)?,
        first_sequence: sequence(first)?,
        last_sequence: sequence(last)?,
        committed_revision: StateRevision::new(sequence(revision)?),
        replayed: true,
    }))
}

#[allow(clippy::too_many_arguments)]
fn append_receipt(
    transaction: &Transaction<'_>,
    ingress: TransitionIngress,
    transaction_id: ActivityTransactionId,
    disposition: RequestDisposition,
    first: u64,
    last: u64,
    revision: StateRevision,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let result = serde_json::to_string(&disposition).map_err(|_| StorageError::InvalidActivity)?;
    let first = i64::try_from(first).map_err(|_| StorageError::InvalidActivity)?;
    let last = i64::try_from(last).map_err(|_| StorageError::InvalidActivity)?;
    let revision = i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?;
    transaction.execute(
        "INSERT INTO pod0_transition_receipts(ingress_code,ingress_id,fingerprint,transaction_id,\
         disposition_code,first_sequence,last_sequence,committed_revision,result_json,committed_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![ingress.kind.code(), ingress.id.as_slice(), ingress.fingerprint.into_bytes().as_slice(),
            transaction_id.into_bytes().as_slice(), disposition_code(disposition), first, last,
            revision, result, committed_at.value],
    ).map_err(|error| StorageError::sqlite("append transition receipt", error))?;
    Ok(())
}

fn append_effect_intent(
    transaction: &Transaction<'_>,
    authorizing: &ActivityFactDraft,
    intent_id: [u8; 16],
    request: DurableExternalEffectRequest,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let (subject_code, subject_id) = subject(request.subject);
    let payload = serde_json::to_string(&request).map_err(|_| StorageError::InvalidActivity)?;
    transaction
        .execute(
            "INSERT INTO pod0_effect_intents(intent_id,authorizing_activity_id,correlation_id,\
         effect_kind_code,subject_code,subject_id,episode_id,request_json,available_at_ms,\
         deadline_at_ms,committed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                intent_id.as_slice(),
                authorizing.activity_id.into_bytes().as_slice(),
                authorizing.correlation_id.into_bytes().as_slice(),
                effect_kind_code(request.kind),
                subject_code,
                subject_id,
                request.episode_id.map(|value| value.into_bytes()),
                payload,
                request.not_before.unwrap_or(committed_at).value,
                request.deadline_at.map(|value| value.value),
                committed_at.value
            ],
        )
        .map_err(|error| StorageError::sqlite("append effect intent", error))?;
    Ok(())
}

fn append_internal_command(
    transaction: &Transaction<'_>,
    authorizing: &ActivityFactDraft,
    command_id: [u8; 16],
    request: DurableInternalCommandRequest,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let (subject_code, subject_id) = subject(request.subject);
    let payload = serde_json::to_string(&request).map_err(|_| StorageError::InvalidActivity)?;
    transaction
        .execute(
            "INSERT INTO pod0_internal_command_intents(internal_command_id,authorizing_activity_id,\
         correlation_id,target_domain_code,subject_code,subject_id,episode_id,command_json,\
         committed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                command_id.as_slice(),
                authorizing.activity_id.into_bytes().as_slice(),
                authorizing.correlation_id.into_bytes().as_slice(),
                domain_code(request.target),
                subject_code,
                subject_id,
                request.episode_id.map(|value| value.into_bytes()),
                payload,
                committed_at.value
            ],
        )
        .map_err(|error| StorageError::sqlite("append internal command", error))?;
    Ok(())
}
