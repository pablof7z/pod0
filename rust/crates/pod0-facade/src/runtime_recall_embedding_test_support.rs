use pod0_application::{
    HostObservation, HostObservationEnvelope, HostObservationReceipt, HostRequest,
    LeasedHostObservationEnvelope, RecallEmbeddingVector, RecallSpanEmbeddingObservation,
};
use pod0_domain::UnixTimestampMilliseconds;

use crate::Pod0Facade;
use crate::runtime_recall_test_support::{recall_test_embedding, record};

pub(super) fn complete_evidence_embedding_requests(facade: &Pod0Facade) {
    loop {
        let leased = facade.next_leased_host_requests(1).pop();
        let request = leased
            .as_ref()
            .map(|value| value.request.clone())
            .or_else(|| facade.next_host_requests(1).pop());
        let Some(request) = request else { break };
        let HostRequest::EmbedRecallSpans {
            episode_id,
            generation_id,
            spans,
            ..
        } = &request.request
        else {
            panic!("expected evidence embedding request")
        };
        let observation = HostObservation::RecallSpansEmbedded {
            episode_id: *episode_id,
            generation_id: *generation_id,
            embeddings: spans
                .iter()
                .map(|span| RecallSpanEmbeddingObservation {
                    span_id: span.span_id,
                    embedding: RecallEmbeddingVector {
                        values: recall_test_embedding(),
                    },
                })
                .collect(),
        };
        if let Some(leased) = leased {
            let receipt = facade.record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: leased.lease,
                observation: HostObservationEnvelope {
                    request_id: request.request_id,
                    cancellation_id: request.cancellation_id,
                    observed_request_revision: request.issued_revision,
                    sequence_number: 0,
                    observed_at: UnixTimestampMilliseconds::new(leased.lease.expires_at.value - 1),
                    observation,
                },
            });
            assert!(matches!(receipt, HostObservationReceipt::Persisted { .. }));
        } else {
            record(facade, &request, observation);
        }
    }
}
