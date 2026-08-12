use crate::*;

pub(crate) fn start_command(id: u64) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(101, id),
        cancellation_id: CancellationId::from_parts(102, id),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Save architecture matters as a note".to_owned(),
            model_reference: "openrouter/test".to_owned(),
        },
    }
}

pub(crate) fn observe(
    request: &HostRequestEnvelope,
    observation: HostObservation,
) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        observation,
    }
}

pub(crate) fn next_leased_agent_request(facade: &Pod0Facade) -> LeasedHostRequestEnvelope {
    facade.next_leased_host_requests(1).remove(0)
}

pub(crate) fn record_leased_agent_observation(
    facade: &Pod0Facade,
    request: &LeasedHostRequestEnvelope,
    observation: HostObservation,
) -> HostObservationReceipt {
    let mut observation = observe(&request.request, observation);
    observation.observed_at = request.lease.expires_at;
    facade.record_leased_host_observation(LeasedHostObservationEnvelope {
        lease: request.lease,
        observation,
    })
}

pub(crate) fn uuid_string(bytes: [u8; 16]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
