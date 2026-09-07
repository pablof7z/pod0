impl EffectOutbox {
    fn claim_next_with_identity(
        &self,
        identity: Option<(EffectAttemptId, EffectLeaseId)>,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
        maximum_active_publisher_chapters: u16,
        defer_core_wakes: bool,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        let duration = i64::from(lease_duration_milliseconds);
        if !(MIN_LEASE_MILLISECONDS..=MAX_LEASE_MILLISECONDS).contains(&duration) {
            return Err(EffectOutboxError::InvalidLeaseDuration);
        }
        let expires_at = now
            .value
            .checked_add(duration)
            .ok_or(EffectOutboxError::InvalidLeaseDuration)?;
        let mut connection = current_connection(&self.path, false)?;
        configure(&connection).map_err(|_| EffectOutboxError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| EffectOutboxError::Storage)?;
        let row = transaction
            .query_row(
                "SELECT i.intent_id,i.authorizing_activity_id,i.correlation_id,i.fence,i.request_json \
                 FROM pod0_effect_intents i WHERE i.effect_kind_code!=14 \
                 AND (i.state_code=1 OR i.effect_kind_code!=10 OR json_extract(i.request_json,\
                 '$.execution.AgentCapability.request.capability.execution_mode')='RecoverExisting') \
                 AND (?3=0 OR i.effect_kind_code!=12 OR COALESCE(json_extract(i.request_json,\
                 '$.execution.Lifecycle.request.wake_at.value'),i.available_at_ms)<=?1) \
                 AND i.available_at_ms<=?1 AND (i.state_code=1 OR \
                 (i.state_code=2 AND NOT EXISTS(SELECT 1 FROM pod0_effect_attempts a \
                 WHERE a.intent_id=i.intent_id AND a.state_code=1 AND \
                 (a.lease_expires_at_ms>?1 OR (a.observed_at_ms IS NOT NULL AND \
                 json_type(i.request_json,'$.execution.Playback.request.action.ObservePlayback') \
                 IS NOT NULL) OR (a.observed_at_ms IS NOT NULL AND i.effect_kind_code=11 AND \
                 EXISTS(SELECT 1 FROM pod0_scheduled_occurrences occurrence \
                 WHERE occurrence.occurrence_id=i.subject_id \
                 AND occurrence.stage='host_accepted')))))) \
                 AND (json_extract(i.request_json,'$.kind')!='PublisherChapterProvider' OR \
                 (SELECT COUNT(*) FROM pod0_effect_attempts active \
                  JOIN pod0_effect_intents owned ON owned.intent_id=active.intent_id \
                  WHERE active.state_code=1 AND active.lease_expires_at_ms>?1 \
                  AND json_extract(owned.request_json,'$.kind')='PublisherChapterProvider')<?2) \
                 ORDER BY i.available_at_ms,i.committed_at_ms,i.rowid LIMIT 1",
                params![
                    now.value,
                    i64::from(maximum_active_publisher_chapters),
                    i64::from(defer_core_wakes)
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| EffectOutboxError::Storage)?;
        let Some((intent, activity, correlation, prior_fence, payload)) = row else {
            return Ok(None);
        };
        let fence = prior_fence
            .checked_add(1)
            .ok_or(EffectOutboxError::InvalidRecord)?;
        let (attempt_id, lease_id) = identity.unwrap_or_else(|| generated_ids(&intent, fence));
        let updated = transaction
            .execute(
                "UPDATE pod0_effect_intents SET state_code=2,fence=?1 \
                 WHERE intent_id=?2 AND fence=?3",
                params![fence, intent.as_slice(), prior_fence],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        if updated != 1 {
            return Err(EffectOutboxError::StaleLease);
        }
        transaction
            .execute(
                "INSERT INTO pod0_effect_attempts(attempt_id,intent_id,lease_id,fence,state_code,\
                 claimed_at_ms,lease_expires_at_ms) VALUES(?1,?2,?3,?4,1,?5,?6)",
                params![
                    attempt_id.into_bytes().as_slice(),
                    intent.as_slice(),
                    lease_id.into_bytes().as_slice(),
                    fence,
                    now.value,
                    expires_at
                ],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        transaction
            .commit()
            .map_err(|_| EffectOutboxError::Storage)?;
        let request: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| EffectOutboxError::InvalidRecord)?;
        let fence = u64::try_from(fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
        Ok(Some(EffectLease {
            intent_id: EffectIntentId::from_bytes(id(&intent)?),
            attempt_id,
            lease_id,
            fence,
            authorizing_activity_id: ActivityId::from_bytes(id(&activity)?),
            correlation_id: ActivityCorrelationId::from_bytes(id(&correlation)?),
            subject: request.subject,
            episode_id: request.episode_id,
            request,
            expires_at: UnixTimestampMilliseconds::new(expires_at),
        }))
       }
}
