use pod0_domain::SignerAccountId;

use crate::runtime_signer::SignerRuntimeAction;
use crate::{FacadeOpenError, Pod0Facade};

impl Pod0Facade {
    pub(super) fn apply_signer_runtime_action(&self, action: SignerRuntimeAction) -> bool {
        match action {
            SignerRuntimeAction::None => false,
            SignerRuntimeAction::Install {
                account_id,
                public_key_hex,
            } => {
                let installed = self
                    .start_nmp()
                    .and_then(|()| {
                        self.nmp
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .as_ref()
                            .ok_or(FacadeOpenError::StorageUnavailable)?
                            .install_native_signer(account_id, &public_key_hex)
                            .map_err(|_| FacadeOpenError::StorageUnavailable)
                    })
                    .is_ok();
                self.state().finalize_signer_install(account_id, installed)
            }
            SignerRuntimeAction::Remove { account_id } => {
                self.detach_native_signer(account_id);
                false
            }
            SignerRuntimeAction::Complete {
                request_id,
                observation,
            } => self
                .nmp
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|runtime| runtime.complete_native_signature(request_id, observation)),
            SignerRuntimeAction::Fail {
                request_id,
                failure,
            } => self
                .nmp
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|runtime| runtime.fail_native_signature(request_id, failure)),
        }
    }

    pub(super) fn detach_native_signer(&self, account_id: SignerAccountId) {
        if let Some(runtime) = self
            .nmp
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            let _ = runtime.remove_native_signer(account_id);
        }
    }
}
