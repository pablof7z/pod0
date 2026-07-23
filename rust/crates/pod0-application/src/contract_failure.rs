#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CoreFailure {
    pub code: CoreFailureCode,
    pub safe_detail: Option<String>,
    pub retryability: Retryability,
    pub user_action: UserAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CoreFailureCode {
    InvalidCommand,
    InvalidFeedUrl,
    FeedMalformed,
    AlreadySubscribed,
    StorageUnavailable,
    RevisionConflict,
    NotFound,
    InvalidMemory,
    InvalidNote,
    InvalidClip,
    InvalidTranscript,
    InvalidChapter,
    HostUnavailable,
    Unauthorized,
    HostRejected,
    Cancelled,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Retryability {
    Never,
    Automatic,
    AfterUserAction,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum UserAction {
    None,
    Retry,
    CheckConnection,
    ReviewPermissions,
    Unsupported { wire_code: u32 },
}
