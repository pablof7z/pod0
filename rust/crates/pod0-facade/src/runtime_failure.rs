use pod0_application::{CoreFailure, CoreFailureCode, Retryability, UserAction};

pub(super) fn failure(code: CoreFailureCode) -> CoreFailure {
    let (retryability, user_action) = match code {
        CoreFailureCode::HostUnavailable | CoreFailureCode::StorageUnavailable => {
            (Retryability::Automatic, UserAction::Retry)
        }
        CoreFailureCode::InvalidFeedUrl
        | CoreFailureCode::FeedMalformed
        | CoreFailureCode::Unauthorized => {
            (Retryability::AfterUserAction, UserAction::ReviewPermissions)
        }
        _ => (Retryability::Never, UserAction::None),
    };
    CoreFailure {
        code,
        safe_detail: None,
        retryability,
        user_action,
    }
}
