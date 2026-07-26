impl FacadeState {
    fn retry_pending_durable_observation(
        &mut self,
        observation: &HostObservationEnvelope,
    ) -> Option<(bool, HostObservationReceipt)> {
        let request_id = observation.request_id;
        if let Some(pending) = self
            .pending_feed_discovery_notification_observations
            .get(&request_id)
            .cloned()
        {
            if &pending != observation {
                return Some((false, retain(request_id)));
            }
            let result = self.persist_feed_discovery_notification_observation(observation.clone());
            if !matches!(result.1, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_feed_discovery_notification_observations
                    .remove(&request_id);
            }
            return Some(result);
        }
        if let Some(pending) = self.pending_download_observations.get(&request_id).cloned() {
            if &pending != observation {
                return Some((false, retain(request_id)));
            }
            let receipt = self.persist_download_observation(observation.clone());
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            if !matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_download_observations.remove(&request_id);
            }
            return Some((changed, receipt));
        }
        None
    }

    pub(super) fn retry_pending_publisher_observations(&mut self) -> bool {
        let request_ids = self
            .pending_publisher_observations
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut changed = false;
        for request_id in request_ids {
            let Some(record) = self.pending_publisher_chapters.get(&request_id).cloned() else {
                self.pending_publisher_observations.remove(&request_id);
                continue;
            };
            let Some(observation) = self
                .pending_publisher_observations
                .get(&request_id)
                .cloned()
            else {
                continue;
            };
            if self.finish_publisher_chapter_observation(record, observation) {
                self.retire_publisher_chapter_request(request_id);
                changed = true;
            }
        }
        if changed {
            self.trim_operations();
        }
        changed
    }
}
