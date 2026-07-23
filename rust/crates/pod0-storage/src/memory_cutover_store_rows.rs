fn stage_rows(
    transaction: &Transaction<'_>,
    input: &LegacyMemoryCutoverInput,
) -> Result<(), StorageError> {
    for memory in &input.memories {
        transaction
            .execute(
                "INSERT INTO pod0_memories(memory_id,memory_revision,content,source_code,\
                 created_at_ms,updated_at_ms,deleted,created_command_id) \
                 VALUES(?1,1,?2,2,?3,?3,?4,NULL)",
                params![
                    memory.memory_id.into_bytes().as_slice(),
                    memory.content,
                    memory.created_at.value(),
                    i64::from(memory.deleted),
                ],
            )
            .map_err(|error| StorageError::sqlite("stage legacy memory", error))?;
    }
    if let Some(compiled) = &input.compiled {
        transaction
            .execute(
                "INSERT INTO pod0_compiled_memory(singleton,text,compiled_at_ms) VALUES(1,?1,?2)",
                params![compiled.text, compiled.compiled_at.value()],
            )
            .map_err(|error| StorageError::sqlite("stage compiled memory", error))?;
        for (index, memory_id) in compiled.source_memory_ids.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO pod0_compiled_memory_sources(singleton,sort_order,memory_id) \
                     VALUES(1,?1,?2)",
                    params![to_i64(index as u64)?, memory_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("stage compiled memory source", error))?;
        }
    }
    Ok(())
}

fn verify_rows(
    transaction: &Transaction<'_>,
    report: &LegacyMemoryCutoverReport,
) -> Result<(), StorageError> {
    let memory_count = count(transaction, "SELECT COUNT(*) FROM pod0_memories")?;
    let deleted = count(
        transaction,
        "SELECT COUNT(*) FROM pod0_memories WHERE deleted=1",
    )?;
    let compiled = count(transaction, "SELECT COUNT(*) FROM pod0_compiled_memory")?;
    if memory_count != u64::from(report.memory_count)
        || deleted != u64::from(report.deleted_count)
        || (compiled == 1) != report.compiled_present
    {
        return Err(StorageError::RevisionConflict);
    }
    Ok(())
}

fn ensure_inactive_empty(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    let active: i64 = transaction
        .query_row(
            "SELECT authority_active FROM pod0_memory_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read memory authority", error))?;
    if active != 0 || count(transaction, "SELECT COUNT(*) FROM pod0_memories")? != 0 {
        return Err(StorageError::RevisionConflict);
    }
    Ok(())
}

fn clear_rows(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction
        .execute("DELETE FROM pod0_compiled_memory", [])
        .map_err(|error| StorageError::sqlite("clear compiled memory", error))?;
    transaction
        .execute("DELETE FROM pod0_memories", [])
        .map_err(|error| StorageError::sqlite("clear staged memories", error))?;
    Ok(())
}

fn matching_report(
    transaction: &Transaction<'_>,
    source_generation: u64,
) -> Result<LegacyMemoryCutoverReport, StorageError> {
    read_evidence(transaction)?
        .filter(|report| report.state.source_generation() == Some(source_generation))
        .ok_or(StorageError::RevisionConflict)
}

fn read_report(connection: &Connection) -> Result<LegacyMemoryCutoverReport, StorageError> {
    Ok(
        read_evidence(connection)?.unwrap_or(LegacyMemoryCutoverReport {
            state: MemoryCutoverState::NotStarted,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            memory_count: 0,
            deleted_count: 0,
            compiled_present: false,
        }),
    )
}

type EvidenceRow = (String, i64, Vec<u8>, Vec<u8>, i64, i64, i64, i64);

fn read_evidence(
    connection: &Connection,
) -> Result<Option<LegacyMemoryCutoverReport>, StorageError> {
    let row: Option<EvidenceRow> = connection
        .query_row(
            "SELECT state,source_generation,source_fingerprint,backup_digest,backup_byte_count,\
             memory_count,deleted_count,compiled_present FROM pod0_memory_cutover_evidence \
             WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read memory cutover evidence", error))?;
    row.map(decode_evidence).transpose()
}

fn decode_evidence(row: EvidenceRow) -> Result<LegacyMemoryCutoverReport, StorageError> {
    let (state, generation, fingerprint, digest, bytes, count, deleted, compiled) = row;
    let generation = unsigned(generation)?;
    let state = match state.as_str() {
        "staged" => MemoryCutoverState::Staged {
            source_generation: generation,
        },
        "verified" => MemoryCutoverState::Verified {
            source_generation: generation,
        },
        "authoritative" => MemoryCutoverState::Authoritative {
            source_generation: generation,
        },
        _ => {
            return Err(StorageError::CorruptSchema {
                detail: "memory cutover state is malformed",
            });
        }
    };
    Ok(LegacyMemoryCutoverReport {
        state,
        source_fingerprint: Some(digest_value(&fingerprint)?),
        backup_digest: Some(digest_value(&digest)?),
        backup_byte_count: Some(unsigned(bytes)?),
        memory_count: u32::try_from(unsigned(count)?).map_err(|_| StorageError::CorruptSchema {
            detail: "memory count is malformed",
        })?,
        deleted_count: u32::try_from(unsigned(deleted)?).map_err(|_| {
            StorageError::CorruptSchema {
                detail: "deleted memory count is malformed",
            }
        })?,
        compiled_present: compiled == 1,
    })
}

fn digest_value(bytes: &[u8]) -> Result<ContentDigest, StorageError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "memory cutover digest is malformed",
    })?;
    Ok(ContentDigest::from_bytes(bytes))
}

fn count(connection: &Connection, sql: &str) -> Result<u64, StorageError> {
    let value: i64 = connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("count memory rows", error))?;
    unsigned(value)
}

fn unsigned(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptSchema {
        detail: "memory cutover integer is malformed",
    })
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidMemory)
}
