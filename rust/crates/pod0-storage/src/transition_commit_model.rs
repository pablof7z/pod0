use pod0_application::RequestDisposition;
use pod0_domain::{ActivityTransactionId, ContentDigest, StateRevision};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransitionIngressKind {
    ApplicationCommand,
    HostObservation,
    InternalCommand,
    ScheduledWake,
    Recovery,
    Migration,
}

impl TransitionIngressKind {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::ApplicationCommand => 1,
            Self::HostObservation => 2,
            Self::InternalCommand => 3,
            Self::ScheduledWake => 4,
            Self::Recovery => 5,
            Self::Migration => 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TransitionIngressKind;

    #[test]
    fn ingress_codes_are_stable_and_migration_is_distinct() {
        assert_eq!(TransitionIngressKind::ApplicationCommand.code(), 1);
        assert_eq!(TransitionIngressKind::HostObservation.code(), 2);
        assert_eq!(TransitionIngressKind::InternalCommand.code(), 3);
        assert_eq!(TransitionIngressKind::ScheduledWake.code(), 4);
        assert_eq!(TransitionIngressKind::Recovery.code(), 5);
        assert_eq!(TransitionIngressKind::Migration.code(), 6);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TransitionIngress {
    pub kind: TransitionIngressKind,
    pub id: [u8; 16],
    pub fingerprint: ContentDigest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    pub transaction_id: ActivityTransactionId,
    pub disposition: RequestDisposition,
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub committed_revision: StateRevision,
    pub replayed: bool,
}
