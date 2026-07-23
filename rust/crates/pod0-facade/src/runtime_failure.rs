use pod0_application::{CoreFailure, CoreFailureCode, Retryability, UserAction};
use pod0_domain::CommandId;

use crate::runtime_state::FacadeState;

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

impl FacadeState {
    pub(super) fn reject_unsupported(&mut self, command_id: CommandId, wire_code: u32) {
        self.fail(command_id, CoreFailureCode::Unsupported { wire_code });
    }
}
