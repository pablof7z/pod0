use super::*;

#[uniffi::export]
impl Pod0Facade {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::from_state(FacadeState::default())
    }

    #[uniffi::constructor]
    pub fn open(store_path: String) -> Result<Arc<Self>, FacadeOpenError> {
        Self::open_with_clock_value(store_path, Arc::new(SystemClock))
    }

    #[uniffi::constructor]
    pub fn create(store_path: String) -> Result<Arc<Self>, FacadeOpenError> {
        let path = Path::new(&store_path);
        if let Some(parent) = path.parent()
            && !pod0_storage::pending_user_data_erasure_markers(parent)
                .map_err(FacadeOpenError::from)?
                .is_empty()
        {
            return Err(FacadeOpenError::ErasureRecoveryRequired);
        }
        let observed_at_ms = current_unix_milliseconds();
        pod0_storage::create_authoritative_store(
            path,
            fresh_store_id(path, observed_at_ms),
            observed_at_ms,
        )
        .map_err(FacadeOpenError::from)?;
        Self::open(store_path)
    }

    pub fn dispatch(&self, command: CommandEnvelope) {
        let cancellation_id = cancellation_target(&command);
        if let Some(cancellation_id) = cancellation_id {
            self.recall_interrupts.signal(cancellation_id);
        }
        let changed = self.state().dispatch(command);
        if let Some(cancellation_id) = cancellation_id {
            self.recall_interrupts.finish_signal(cancellation_id);
        }
        if changed {
            self.notify_subscribers();
        }
    }

    pub fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        self.state().snapshot(request)
    }

    /// Plans the exact bounded chapter-model capability request from the
    /// authoritative Rust episode, transcript, and chapter selections.
    pub fn plan_chapter_model_request(
        &self,
        episode_id: pod0_domain::EpisodeId,
        configured_model: String,
    ) -> pod0_application::ChapterModelPlan {
        self.chapter_model_plan(episode_id, configured_model)
    }

    /// Reads the secret-free Rust-owned workflow policy for exact-revision
    /// native setting updates. Absence means the one-time import has not yet
    /// established authority.
    pub fn workflow_configuration(
        &self,
    ) -> Result<Option<pod0_application::WorkflowConfiguration>, FacadeOpenError> {
        let store = self
            .state()
            .store
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)?;
        store
            .workflow_configuration()
            .map_err(FacadeOpenError::from)
    }

    pub fn subscribe(
        &self,
        request: ProjectionRequest,
        subscriber: Arc<dyn ProjectionSubscriber>,
    ) -> SubscriptionId {
        let (subscription_id, projection) = {
            let mut state = self.state();
            let id = state.subscriptions.subscribe(request);
            state.subscribers.insert(id, Arc::clone(&subscriber));
            let projection = state.snapshot(request);
            let content = state.delivery_content(request, &projection.projection);
            state
                .delivered_projections
                .insert(id, projection.projection.clone());
            state.delivered_contents.insert(id, content);
            (id, projection)
        };
        subscriber.receive(projection);
        subscription_id
    }

    pub fn unsubscribe(&self, subscription_id: SubscriptionId) {
        let mut state = self.state();
        let _ = state.subscriptions.unsubscribe(subscription_id);
        state.subscribers.remove(&subscription_id);
        state.delivered_projections.remove(&subscription_id);
        state.delivered_contents.remove(&subscription_id);
    }

    pub fn next_leased_host_requests(
        &self,
        maximum_count: u16,
    ) -> Vec<pod0_application::LeasedHostRequestEnvelope> {
        let (changed, requests) = self.state().next_leased_transcript_requests(maximum_count);
        if changed {
            self.notify_subscribers();
        }
        requests
    }

    pub fn record_leased_host_observation(
        &self,
        observation: pod0_application::LeasedHostObservationEnvelope,
    ) -> pod0_application::HostObservationReceipt {
        let (changed, receipt) = self.state().record_leased_host_observation(observation);
        if changed {
            self.notify_subscribers();
        }
        receipt
    }
}

fn cancellation_target(command: &CommandEnvelope) -> Option<CancellationId> {
    match command.command {
        ApplicationCommand::CancelOperation { cancellation_id } => Some(cancellation_id),
        _ => None,
    }
}

fn current_unix_milliseconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn fresh_store_id(path: &Path, observed_at_ms: i64) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0:fresh-store-id:v1\0");
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update(observed_at_ms.to_be_bytes());
    hash.update(std::process::id().to_be_bytes());
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}
