//! Typed failures for opening the authoritative store, and the reason a
//! blocked store cannot be opened.

/// Why a blocked store cannot be opened, in terms a recovery surface can act
/// on. Six distinct `StorageError` cases collapse into three remedies; the
/// host needs the remedy, not the cause.
///
/// None of these are fixed by relaunching. That is the point of carrying the
/// reason: without it the host can only offer one recovery instruction, and
/// for every case here "close and reopen" is false.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SchemaBlockReason {
    /// The store was written by a newer build than this one. Downgrade is
    /// refused, so the store is never rewritten back down — the only remedy
    /// is a build at least as new as the data.
    StoreNewerThanApp,
    /// A migration started and did not finish. A verified backup was taken
    /// before it began.
    MigrationFailed,
    /// The store is corrupt, or belongs to another application.
    StoreUnreadable,
}

#[derive(Debug, uniffi::Error)]
pub enum FacadeOpenError {
    NotAuthoritative,
    SchemaBlocked { reason: SchemaBlockReason },
    StorageUnavailable,
}

impl std::fmt::Display for FacadeOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotAuthoritative => "shared listening store is not authoritative",
            Self::SchemaBlocked {
                reason: SchemaBlockReason::StoreNewerThanApp,
            } => "shared listening store was written by a newer build",
            Self::SchemaBlocked {
                reason: SchemaBlockReason::MigrationFailed,
            } => "shared listening store has an unfinished migration",
            Self::SchemaBlocked {
                reason: SchemaBlockReason::StoreUnreadable,
            } => "shared listening store is unreadable",
            Self::StorageUnavailable => "shared listening store is unavailable",
        })
    }
}

impl std::error::Error for FacadeOpenError {}
