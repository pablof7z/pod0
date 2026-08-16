use pod0_application::CommittedActivityFact;

pub const MAX_ACTIVITY_PAGE_ITEMS: u16 = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityPage {
    pub items: Vec<CommittedActivityFact>,
    pub next_after_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatestActivityPage {
    pub items: Vec<CommittedActivityFact>,
    pub snapshot_through_sequence: Option<u64>,
    pub next_before_sequence: Option<u64>,
}
