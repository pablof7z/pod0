use pod0_domain::{CommandId, ListeningDomainSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacySourceKind {
    SwiftSqlite,
    LegacyJson,
}

impl LegacySourceKind {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::SwiftSqlite => 1,
            Self::LegacyJson => 2,
        }
    }

    pub(crate) const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::SwiftSqlite),
            2 => Some(Self::LegacyJson),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyImportPlan {
    pub source_kind: LegacySourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub podcast_count: u32,
    pub subscription_count: u32,
    pub episode_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyBackupEvidence {
    pub source_kind: LegacySourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub byte_count: u64,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListeningImportReport {
    pub import_id: CommandId,
    pub plan: LegacyImportPlan,
    pub target_revision: u64,
    pub backup: LegacyBackupEvidence,
    pub staged: bool,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListeningImportVerification {
    pub report: ListeningImportReport,
    pub snapshot: ListeningDomainSnapshot,
}

pub(crate) struct InspectedLegacySource {
    pub(crate) plan: LegacyImportPlan,
    pub(crate) snapshot: ListeningDomainSnapshot,
    pub(crate) episode_payloads: Vec<Vec<u8>>,
}
